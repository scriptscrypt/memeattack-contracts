use anchor_lang::prelude::*;
use std::collections::HashMap;
use borsh::{BorshDeserialize, BorshSerialize};

declare_id!("FrheP2MW6jDuSrvKVyAjQeCjrY8VuitWLmgxRaDeCKWh");

#[program]
pub mod meme_coin_game {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, initial_prize_per_box: u64) -> Result<()> {
        let total_prize = initial_prize_per_box * 9;

        for i in 0..9 {
            ctx.accounts.game_state.boxes[i] = GameBox {
                amount_in_lamports: initial_prize_per_box,
                ..Default::default()
            };
        }

        ctx.accounts.game_state.prize_pool = total_prize;

        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to: ctx.accounts.game_state.to_account_info(),
                },
            ),
            total_prize,
        )?;

        Ok(())
    }

    pub fn enter_game(
        ctx: Context<EnterGame>,
        meme_coin_name: String,
        amount_in_lamports: u64,
        box_number: u8,
    ) -> Result<()> {
        require!(box_number < 9, ErrorCode::InvalidBoxNumber);
        require!(amount_in_lamports > 0, ErrorCode::InvalidAmount);

        let clock = Clock::get()?;
        let box_entry = &mut ctx.accounts.game_state.boxes[box_number as usize];

        if box_entry.meme_coin_name == meme_coin_name {
            box_entry.amount_in_lamports += amount_in_lamports;
        } else {
            let new_total = box_entry.amount_in_lamports + amount_in_lamports;
            if amount_in_lamports > box_entry.amount_in_lamports {
                box_entry.meme_coin_name = meme_coin_name;
                box_entry.amount_in_lamports = new_total;
                box_entry.start_time = clock.unix_timestamp;
                box_entry.contributions.clear();
            } else {
                box_entry.amount_in_lamports = new_total;
            }
        }

        *box_entry
            .contributions
            .entry(ctx.accounts.player.key())
            .or_insert(0) += amount_in_lamports;

        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.game_state.to_account_info(),
                },
            ),
            amount_in_lamports,
        )?;

        ctx.accounts.game_state.prize_pool += amount_in_lamports;

        Ok(())
    }

    pub fn claim_prize(
        ctx: Context<ClaimPrize>,
        box_number: u8,
        meme_coin_name: String,
    ) -> Result<()> {
        require!(box_number < 9, ErrorCode::InvalidBoxNumber);

        let game_state = &mut ctx.accounts.game_state;
        let box_entry = &game_state.boxes[box_number as usize];
        require!(box_entry.meme_coin_name == meme_coin_name, ErrorCode::NotBoxOwner);

        let clock = Clock::get()?;
        let time_elapsed = clock.unix_timestamp - box_entry.start_time;
        require!(time_elapsed >= 3600, ErrorCode::TimeNotElapsed);

        let total_prize = box_entry.amount_in_lamports;
        let player_key = ctx.accounts.player.key();
        let player_contribution = *box_entry.contributions.get(&player_key).unwrap_or(&0);
        let player_share = ((player_contribution as u128) * (total_prize as u128) / (box_entry.amount_in_lamports as u128)) as u64;

        let box_entry = &mut game_state.boxes[box_number as usize];
        box_entry.contributions.remove(&player_key);
        box_entry.amount_in_lamports -= player_contribution;

        if box_entry.contributions.is_empty() {
            *box_entry = GameBox::default();
        }

        game_state.prize_pool -= player_share;

        **ctx.accounts.game_state.to_account_info().try_borrow_mut_lamports()? -= player_share;
        **ctx.accounts.player.to_account_info().try_borrow_mut_lamports()? += player_share;

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = user, space = 8 + GameState::LEN)]
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
    pub boxes: [GameBox; 9],
    pub prize_pool: u64,
}

impl GameState {
    pub const LEN: usize = 8 + (9 * GameBox::LEN) + 8;
}

#[derive(Clone, Default)]
pub struct GameBox {
    pub meme_coin_name: String,
    pub amount_in_lamports: u64,
    pub start_time: i64,
    pub contributions: HashMap<Pubkey, u64>,
}

impl BorshSerialize for GameBox {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.meme_coin_name.serialize(writer)?;
        self.amount_in_lamports.serialize(writer)?;
        self.start_time.serialize(writer)?;
        self.contributions.serialize(writer)?;
        Ok(())
    }
}

impl BorshDeserialize for GameBox {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let meme_coin_name = String::deserialize_reader(reader)?;
        let amount_in_lamports = u64::deserialize_reader(reader)?;
        let start_time = i64::deserialize_reader(reader)?;
        let contributions = HashMap::<Pubkey, u64>::deserialize_reader(reader)?;

        Ok(GameBox {
            meme_coin_name,
            amount_in_lamports,
            start_time,
            contributions,
        })
    }
}

impl GameBox {
    pub const LEN: usize = 32 + 8 + 8 + 32 * 8 + 8;
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