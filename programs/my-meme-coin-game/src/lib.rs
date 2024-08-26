use anchor_lang::prelude::*;
use std::cmp::Ordering;

declare_id!("CkrDU8u3B4fehXLzNPvDKpbbjZ5fAWt6bDp3t6j9prXj");

#[program]
pub mod meme_coin_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        game_state.boxes = [Box::default(); 9];
        game_state.prize_pool = 0;
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

        match box_entry.amount_in_lamports.cmp(&amount_in_lamports) {
            Ordering::Less => {
                // New entry overwrites the existing one
                box_entry.meme_coin_name = meme_coin_name;
                box_entry.deposited_by = player.key();
                box_entry.amount_in_lamports = amount_in_lamports;
                box_entry.start_time = clock.unix_timestamp;
            }
            Ordering::Equal | Ordering::Greater => {
                return Err(ErrorCode::InsufficientAmount.into());
            }
        }

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

    pub fn claim_prize(ctx: Context<ClaimPrize>, box_number: u8) -> Result<()> {
        let game_state = &mut ctx.accounts.game_state;
        let player = &ctx.accounts.player;
        let clock = Clock::get()?;

        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let box_entry = &mut game_state.boxes[box_number as usize];

        require!(box_entry.deposited_by == player.key(), ErrorCode::NotBoxOwner);

        let time_elapsed = clock.unix_timestamp - box_entry.start_time;
        require!(time_elapsed >= 3600, ErrorCode::TimeNotElapsed); // 3600 seconds = 1 hour

        // Transfer the prize (1 SOL) to the winner
        let prize_amount = 1_000_000_000; // 1 SOL in lamports
        **game_state.to_account_info().try_borrow_mut_lamports()? -= prize_amount;
        **player.to_account_info().try_borrow_mut_lamports()? += prize_amount;

        // Reset the box
        *box_entry = Box::default();

        // Reduce prize pool
        game_state.prize_pool -= prize_amount;

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
    pub deposited_by: Pubkey,
    pub amount_in_lamports: u64,
    pub start_time: i64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid box number")]
    InvalidBoxNumber,
    #[msg("Invalid amount")]
    InvalidAmount,
    #[msg("Insufficient amount to replace existing entry")]
    InsufficientAmount,
    #[msg("Not the box owner")]
    NotBoxOwner,
    #[msg("60 minutes have not elapsed yet")]
    TimeNotElapsed,
}