use anchor_lang::prelude::*;

declare_id!("CkrDU8u3B4fehXLzNPvDKpbbjZ5fAWt6bDp3t6j9prXj");

#[program]
pub mod meme_coin_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, initial_prize_per_box: u64) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        let game_id = &mut ctx.accounts.game_id;
        let user = &ctx.accounts.user;

        // Calculate the total required SOL
        let total_required = initial_prize_per_box * 9;

        // Check if the user has enough SOL
        require!(
            user.lamports() >= total_required,
            ErrorCode::InsufficientFunds
        );

        game_state.boxes = vec![Box::default(); 9];
        game_state.prize_pool = initial_prize_per_box * 9;
        game_state.game_id = game_id.key();


        // Initialize each box with the initial prize
        for box_entry in game_state.boxes.iter_mut() {
            box_entry.amount_in_lamports = initial_prize_per_box;
        }

        // Transfer the initial prize pool from the initializer
        let prize_pool = game_state.prize_pool;
        let transfer_instruction = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.game_state.key(),
            prize_pool,
        );
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.game_state.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        Ok(())
    }

    pub fn enter_game(
        ctx: Context<EnterGame>,
        meme_coin_name: String,
        amount_in_lamports: u64,
        box_number: u8,
    ) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        let player = &ctx.accounts.player;
        let clock = Clock::get()?;

        require!(box_number < 9, ErrorCode::InvalidBoxNumber);
        require!(amount_in_lamports > 0, ErrorCode::InvalidAmount);

        let box_entry = &mut game_state.boxes[box_number as usize];

        if box_entry.meme_coin_name == meme_coin_name {
            // Same memecoin, just add to the amount
            box_entry.amount_in_lamports += amount_in_lamports;
            // Don't reset the timer for the same memecoin
        } else {
            // Different memecoin
            let new_total = box_entry.amount_in_lamports + amount_in_lamports;
            if new_total > box_entry.amount_in_lamports {
                // New memecoin takes the lead
                box_entry.meme_coin_name = meme_coin_name;
                box_entry.amount_in_lamports = new_total;
                box_entry.start_time = clock.unix_timestamp; // Reset the timer
                box_entry.contributions.clear(); // Clear previous contributions
            } else {
                // Just add to the total without changing the leader
                box_entry.amount_in_lamports = new_total;
            }
        }

        // Update the player's contribution
        if let Some(contribution) = box_entry.contributions.iter_mut().find(|c| c.contributor == player.key()) {
            contribution.amount += amount_in_lamports;
        } else {
            box_entry.contributions.push(Contribution {
                contributor: player.key(),
                amount: amount_in_lamports,
            });
        }

        // Transfer SOL from player to the program account

        let transfer_instruction = anchor_lang::solana_program::system_instruction::transfer(
            &player.key(),
            &game_state.key(),
            amount_in_lamports,
        );
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                player.to_account_info(),
                game_state.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        game_state.prize_pool += amount_in_lamports;

        Ok(())
    }

    pub fn claim_prize(
        ctx: Context<ClaimPrize>,
        box_number: u8,
        meme_coin_name: String,
    ) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        let player = &ctx.accounts.player;
        let clock = Clock::get()?;

        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let box_entry = &mut game_state.boxes[box_number as usize];

        require!(
            box_entry.meme_coin_name == meme_coin_name,
            ErrorCode::NotBoxOwner
        );

        let time_elapsed = clock.unix_timestamp - box_entry.start_time;
        //require!(time_elapsed >= 3600, ErrorCode::TimeNotElapsed);

        // Calculate the prize amount based on the box's current amount
        let total_prize = box_entry.amount_in_lamports;

        // Calculate the player's share of the prize
        let player_contribution = box_entry.contributions
        .iter()
        .find(|c| c.contributor == player.key())
        .map(|c| c.amount)
        .unwrap_or(0);
        let player_share = (player_contribution as u128 * total_prize as u128
            / box_entry.amount_in_lamports as u128) as u64;

        //Ensure the player has made a contribution
        require!(player_share > 0, ErrorCode::NoContribution);

        // Transfer the prize share to the winner
        let prize_share = player_share;

        // Remove the player's contribution
        box_entry.contributions.retain(|c| c.contributor != player.key());
        box_entry.amount_in_lamports -= player_contribution;

        // If all contributions have been claimed, reset the box
        if box_entry.contributions.is_empty() {
            *box_entry = Box::default();
        }

        // Reduce prize pool
        game_state.prize_pool -= prize_share;

        // Transfer lamports
        **ctx
            .accounts
            .game_state
            .to_account_info()
            .try_borrow_mut_lamports()? -= prize_share;
        **ctx
            .accounts
            .player
            .to_account_info()
            .try_borrow_mut_lamports()? += prize_share;

        Ok(())
    }
}

// Initially, there is no account, we need to make sure user pays for the account creation (payer=user)
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = user,
        space = 8 + // discriminator
                4 + // Vec length prefix
                (9 * ( // 9 boxes
                    32 + // meme_coin_name (String)
                    8 + // amount_in_lamports
                    8 + // start_time
                    4 + // HashMap length prefix
                    (10 * (32 + 8)) // max 10 contributions (Pubkey + u64)
                )) +
                8 + // prize_pool
                32, // game_id (Pubkey)
        seeds = [b"game-state", game_id.key().as_ref()],
        bump
    )]
    pub game_state: Account<'info, GameState>,
    /// CHECK: This account is used as a seed for the PDA
    pub game_id: AccountInfo<'info>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EnterGame<'info> {
    #[account(
        mut,
        seeds = [b"game-state", game_state.game_id.key().as_ref()],
        bump
    )]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(
        mut,
        seeds = [b"game-state", game_state.game_id.key().as_ref()],
        bump
    )]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub player: Signer<'info>,
}

#[account]
#[derive(Debug)]
pub struct GameState {
    pub boxes: Vec<Box>,
    pub prize_pool: u64,
    pub game_id: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default, Debug)]
pub struct Box {
    pub meme_coin_name: String,
    pub amount_in_lamports: u64,
    pub start_time: i64,
    pub contributions: Vec<Contribution>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Contribution {
    pub contributor: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Insufficient funds to initialize the game")]
    InsufficientFunds,
    #[msg("Invalid box number")]
    InvalidBoxNumber,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Not the leading memecoin")]
    NotBoxOwner,
    #[msg("No contribution found for the player")]
    NoContribution,
    #[msg("60 minutes have not elapsed yet")]
    TimeNotElapsed,
}

