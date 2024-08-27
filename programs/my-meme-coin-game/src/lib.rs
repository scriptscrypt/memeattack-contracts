use anchor_lang::prelude::*;
use std::collections::HashMap;

declare_id!("CkrDU8u3B4fehXLzNPvDKpbbjZ5fAWt6bDp3t6j9prXj");

#[program]
pub mod meme_coin_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, initial_prize_per_box: u64) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        game_state.boxes = [Box::default(); 9];
        game_state.prize_pool = initial_prize_per_box * 9;

        // Initialize each box with the initial prize
        for box_entry in game_state.boxes.iter_mut() {
            box_entry.amount_in_lamports = initial_prize_per_box;
        }

        // Transfer the initial prize pool from the initializer
        let transfer_instruction = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.game_state.key(),
            game_state.prize_pool,
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
        *box_entry.contributions.entry(player.key()).or_insert(0) += amount_in_lamports;

        // Transfer SOL from player to the program account
        let transfer_instruction = anchor_lang::solana_program::system_instruction::transfer(
            &player.key(),
            &ctx.accounts.game_state.key(),
            amount_in_lamports,
        );
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                player.to_account_info(),
                ctx.accounts.game_state.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        game_state.prize_pool += amount_in_lamports;

        Ok(())
    }

    pub fn claim_prize(ctx: Context<ClaimPrize>, box_number: u8, meme_coin_name: String) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        let player = &ctx.accounts.player;
        let clock = Clock::get()?;

        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let box_entry = &mut game_state.boxes[box_number as usize];

        require!(box_entry.meme_coin_name == meme_coin_name, ErrorCode::NotBoxOwner);

        let time_elapsed = clock.unix_timestamp - box_entry.start_time;
        require!(time_elapsed >= 3600, ErrorCode::TimeNotElapsed); // 3600 seconds = 1 hour

        // Calculate the prize amount based on the box's current amount
        let total_prize = box_entry.amount_in_lamports;

        // Calculate the player's share of the prize
        let player_contribution = box_entry.contributions.get(&player.key()).unwrap_or(&0);
        let player_share = ((*player_contribution as u128) * (total_prize as u128) / (box_entry.amount_in_lamports as u128)) as u64;

        // Transfer the prize share to the winner
        **game_state.to_account_info().try_borrow_mut_lamports()? -= player_share;
        **player.to_account_info().try_borrow_mut_lamports()? += player_share;

        // Remove the player's contribution
        box_entry.contributions.remove(&player.key());
        box_entry.amount_in_lamports -= *player_contribution;

        // If all contributions have been claimed, reset the box
        if box_entry.contributions.is_empty() {
            *box_entry = Box::default();
        }

        // Reduce prize pool
        game_state.prize_pool -= player_share;

        Ok(())
    }
}

// Initially, there is no account, we need to make sure user pays for the account creation (payer=user)
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + 9 * (32 + 32 + 8 + 8) + 8)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EnterGame<'info> {
    #[account(mut)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimPrize<'info> {
    #[account(mut)]
    pub game_state: Account<'info, GameState>,
    #[account(mut)]
    pub player: Signer<'info>,
}

#[account]
pub struct GameState {
    pub boxes: [Box; 9],
    pub prize_pool: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct Box {
    pub meme_coin_name: String,
    pub amount_in_lamports: u64,
    pub start_time: i64,
    pub contributions: HashMap<Pubkey, u64>,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid box number")]
    InvalidBoxNumber,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Not the leading memecoin")]
    NotBoxOwner,
    #[msg("60 minutes have not elapsed yet")]
    TimeNotElapsed,
}