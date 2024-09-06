use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use solana_program::instruction::Instruction;

declare_id!("CkrDU8u3B4fehXLzNPvDKpbbjZ5fAWt6bDp3t6j9prXj");

// Raydium program ID
pub const RAYDIUM_SWAP_PROGRAM_ID_MAINNET: Pubkey = solana_program::pubkey!("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8");
pub const RAYDIUM_SWAP_PROGRAM_ID_DEVNET: Pubkey = solana_program::pubkey!("HWy1jotHpo6UqeQxx49dpYYdQB8wj9Qk9MdxwjLvDHB8");

#[program]
pub mod meme_box_game {
    use super::*;

    pub fn initialize_game(ctx: Context<InitializeGame>) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        game_state.boxes = vec![Box::default(); 9];
        Ok(())
    }

    pub fn create_box(ctx: Context<CreateBox>, box_number: u8, token_mint: Pubkey) -> Result<()> {
        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let game_state = &mut ctx.accounts.game_state;
        let box_entry = &mut game_state.boxes[box_number as usize];

        require!(box_entry.start_time == 0, ErrorCode::BoxAlreadyExists);

        box_entry.token_mint = token_mint;
        box_entry.start_time = Clock::get()?.unix_timestamp;
        box_entry.last_leader_change_time = box_entry.start_time;

        Ok(())
    }

    pub fn contribute(ctx: Context<Contribute>, box_number: u8, amount: u64) -> Result<()> {
        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let game_state = &mut ctx.accounts.game_state;
        let box_entry = &mut game_state.boxes[box_number as usize];

        require!(box_entry.start_time != 0, ErrorCode::BoxDoesNotExist);

        // Transfer tokens from user to box token account
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.box_token_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Update box state
        box_entry.total_amount += amount;

        // Update contributions
        let user_pubkey = ctx.accounts.user.key();
        let token_mint = ctx.accounts.user_token_account.mint;
        if let Some(contribution) = box_entry.contributions.iter_mut().find(|c| c.user == user_pubkey && c.token_mint == token_mint) {
            contribution.amount += amount;
        } else {
            box_entry.contributions.push(TokenContribution {
                user: user_pubkey,
                token_mint,
                amount,
            });
        }

        // Update leader if necessary
        let current_time = Clock::get()?.unix_timestamp;
        let token_total = box_entry.contributions.iter()
            .filter(|c| c.token_mint == token_mint)
            .map(|c| c.amount)
            .sum::<u64>();

        if token_total > box_entry.current_leader.amount {
            if box_entry.current_leader.amount > 0 {
                box_entry.previous_leaders.push(box_entry.current_leader.clone());
            }
            box_entry.current_leader = Leader {
                token_mint,
                amount: token_total,
            };
            box_entry.last_leader_change_time = current_time;
        }

        Ok(())
    }

    pub fn process_rewards(ctx: Context<ProcessRewards>, box_number: u8) -> Result<()> {
        require!(box_number < 9, ErrorCode::InvalidBoxNumber);
    
        let game_state = &mut ctx.accounts.game_state;
        let box_entry = &mut game_state.boxes[box_number as usize];
        let clock = Clock::get()?;
        let time_elapsed = clock.unix_timestamp - box_entry.last_leader_change_time;
        require!(time_elapsed >= 3600, ErrorCode::TimeNotElapsed);
    
        // Calculate rewards based on contributions
        let total_prize = box_entry.total_amount;
        let winning_token_mint = box_entry.current_leader.token_mint;
    
        // Clone the necessary data to avoid borrowing issues
        let contributions = box_entry.contributions.clone();
        let winning_amount = box_entry.current_leader.amount;
    
        // Drop the mutable borrow of game_state
        drop(game_state);
    
        // Perform swaps for each winning contribution
        for contribution in contributions.iter().filter(|c| c.token_mint == winning_token_mint) {
            let contributor_share = ((contribution.amount as u128 * total_prize as u128) / (winning_amount as u128)) as u64;
    
            // Perform on-chain swap using Raydium
            raydium_swap(
                &ctx.accounts,
                contributor_share,
                contribution.user,
            )?;
        }
    
        // Re-borrow game_state as mutable
        let game_state = &mut ctx.accounts.game_state;
        let box_entry = &mut game_state.boxes[box_number as usize];
    
        // Reset the box after rewards distribution
        *box_entry = Box::default();
    
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeGame<'info> {
    #[account(init, payer = user, space = 8 + 9 * 200)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateBox<'info> {
    #[account(mut)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct Contribute<'info> {
    #[account(mut)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub box_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct ProcessRewards<'info> {
    #[account(mut)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub box_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub recipient: SystemAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    #[account(mut)]
    pub user_account: AccountInfo<'info>, // Add this line
    /// CHECK: This is not dangerous because we don't read or write from this account
    pub raydium_program: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub amm_id: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub amm_authority: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub amm_open_orders: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub amm_target_orders: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub pool_coin_token_account: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub pool_pc_token_account: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_program: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_market: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_bids: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_asks: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_event_queue: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_coin_vault: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_pc_vault: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub serum_vault_signer: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub user_source_token_account: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub user_destination_token_account: AccountInfo<'info>,
    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(mut)]
    pub user_source_owner: AccountInfo<'info>,
}

#[account]
pub struct GameState {
    pub boxes: Vec<Box>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Box {
    pub token_mint: Pubkey,
    pub start_time: i64,
    pub last_leader_change_time: i64,
    pub total_amount: u64,
    pub contributions: Vec<TokenContribution>,
    pub current_leader: Leader,
    pub previous_leaders: Vec<Leader>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct TokenContribution {
    pub user: Pubkey,
    pub token_mint: Pubkey,
    pub amount: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Leader {
    pub token_mint: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid box number")]
    InvalidBoxNumber,
    #[msg("Box already exists")]
    BoxAlreadyExists,
    #[msg("Box does not exist")]
    BoxDoesNotExist,
    #[msg("Not enough time has elapsed")]
    TimeNotElapsed,
}

fn raydium_swap(
    accounts: &ProcessRewards,
    amount_in: u64,
    user: Pubkey,
) -> Result<()> {
    let ix = Instruction {
        program_id: RAYDIUM_SWAP_PROGRAM_ID_DEVNET,
        accounts: vec![
            AccountMeta::new(user, true),
            AccountMeta::new(accounts.amm_id.key(), false),
            AccountMeta::new(accounts.amm_authority.key(), false),
            AccountMeta::new(accounts.amm_open_orders.key(), false),
            AccountMeta::new(accounts.amm_target_orders.key(), false),
            AccountMeta::new(accounts.pool_coin_token_account.key(), false),
            AccountMeta::new(accounts.pool_pc_token_account.key(), false),
            AccountMeta::new(accounts.serum_program.key(), false),
            AccountMeta::new(accounts.serum_market.key(), false),
            AccountMeta::new(accounts.serum_bids.key(), false),
            AccountMeta::new(accounts.serum_asks.key(), false),
            AccountMeta::new(accounts.serum_event_queue.key(), false),
            AccountMeta::new(accounts.serum_coin_vault.key(), false),
            AccountMeta::new(accounts.serum_pc_vault.key(), false),
            AccountMeta::new(accounts.serum_vault_signer.key(), false),
            AccountMeta::new(accounts.user_source_token_account.key(), false),
            AccountMeta::new(accounts.user_destination_token_account.key(), false),
            AccountMeta::new(accounts.user_source_owner.key(), true),
            AccountMeta::new_readonly(accounts.token_program.key(), false),
        ],
        data: raydium_swap_instruction_data(amount_in),
    };

    solana_program::program::invoke(
        &ix,
        &[
            accounts.amm_id.clone(),
            accounts.amm_authority.clone(),
            accounts.amm_open_orders.clone(),
            accounts.amm_target_orders.clone(),
            accounts.pool_coin_token_account.clone(),
            accounts.pool_pc_token_account.clone(),
            accounts.serum_program.clone(),
            accounts.serum_market.clone(),
            accounts.serum_bids.clone(),
            accounts.serum_asks.clone(),
            accounts.serum_event_queue.clone(),
            accounts.serum_coin_vault.clone(),
            accounts.serum_pc_vault.clone(),
            accounts.serum_vault_signer.clone(),
            accounts.user_source_token_account.clone(),
            accounts.user_destination_token_account.clone(),
            accounts.user_source_owner.clone(), // This should now be the user's account
            accounts.token_program.to_account_info(),
        ],
    )?;

    Ok(())
}

fn raydium_swap_instruction_data(amount_in: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(9);
    data.push(9); // Instruction code for swap
    data.extend_from_slice(&amount_in.to_le_bytes());
    data
}