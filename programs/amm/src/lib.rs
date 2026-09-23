use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{
        self, mint_to, transfer_checked, Burn, Mint, MintTo, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

declare_id!("ANvcMRJtkyQPkEwRp8wf7eC8zWyCXU5QTbts9aE5cmPw");

const POOL_SEED: &[u8] = b"pool";
const POSITION_SEED: &[u8] = b"position";
const MAX_FEE_BPS: u16 = 1_000;
const BPS: u128 = 10_000;

#[program]
pub mod amm {
    use super::*;

    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        fee_bps: u16,
        initial_a: u64,
        initial_b: u64,
    ) -> Result<()> {
        require!(initial_a > 0 && initial_b > 0, AmmError::InvalidAmount);
        require!(fee_bps <= MAX_FEE_BPS, AmmError::InvalidFee);
        require_keys_neq!(ctx.accounts.mint_a.key(), ctx.accounts.mint_b.key());

        ctx.accounts.pool.authority = ctx.accounts.authority.key();
        ctx.accounts.pool.mint_a = ctx.accounts.mint_a.key();
        ctx.accounts.pool.mint_b = ctx.accounts.mint_b.key();
        ctx.accounts.pool.treasury = ctx.accounts.treasury.key();
        ctx.accounts.pool.fee_bps = fee_bps;
        ctx.accounts.pool.total_shares = 0;
        ctx.accounts.pool.bump = ctx.bumps.pool;

        deposit_tokens(
            &ctx.accounts.authority,
            &*ctx.accounts.authority_ata_a,
            &*ctx.accounts.vault_a,
            &ctx.accounts.mint_a,
            &ctx.accounts.token_program,
            initial_a,
        )?;
        deposit_tokens(
            &ctx.accounts.authority,
            &*ctx.accounts.authority_ata_b,
            &*ctx.accounts.vault_b,
            &ctx.accounts.mint_b,
            &ctx.accounts.token_program,
            initial_b,
        )?;

        let shares = integer_sqrt((initial_a as u128) * (initial_b as u128))?;
        require!(shares > 0, AmmError::InsufficientLiquidity);
        mint_lp(
            &ctx.accounts.pool,
            &ctx.accounts.lp_mint,
            &ctx.accounts.authority_lp_ata,
            &ctx.accounts.token_program,
            shares as u64,
        )?;
        ctx.accounts.pool.total_shares = shares as u64;
        ctx.accounts.position.owner = ctx.accounts.authority.key();
        ctx.accounts.position.pool = ctx.accounts.pool.key();
        ctx.accounts.position.shares = shares as u64;
        ctx.accounts.position.bump = ctx.bumps.position;
        Ok(())
    }

    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        amount_a: u64,
        amount_b: u64,
        min_shares: u64,
    ) -> Result<()> {
        require!(amount_a > 0 && amount_b > 0, AmmError::InvalidAmount);
        let total_shares = ctx.accounts.pool.total_shares;
        let pool_key = ctx.accounts.pool.key();
        let reserve_a = ctx.accounts.vault_a.amount;
        let reserve_b = ctx.accounts.vault_b.amount;
        let shares = if total_shares == 0 {
            integer_sqrt((amount_a as u128) * (amount_b as u128))?
        } else {
            let shares_a = (amount_a as u128)
                .checked_mul(total_shares as u128)
                .ok_or(AmmError::MathOverflow)?
                / reserve_a as u128;
            let shares_b = (amount_b as u128)
                .checked_mul(total_shares as u128)
                .ok_or(AmmError::MathOverflow)?
                / reserve_b as u128;
            shares_a.min(shares_b)
        };
        require!(
            shares >= min_shares as u128 && shares > 0,
            AmmError::SlippageExceeded
        );

        deposit_tokens(
            &ctx.accounts.user,
            &ctx.accounts.user_ata_a,
            &ctx.accounts.vault_a,
            &ctx.accounts.mint_a,
            &ctx.accounts.token_program,
            amount_a,
        )?;
        deposit_tokens(
            &ctx.accounts.user,
            &ctx.accounts.user_ata_b,
            &ctx.accounts.vault_b,
            &ctx.accounts.mint_b,
            &ctx.accounts.token_program,
            amount_b,
        )?;
        mint_lp(
            &ctx.accounts.pool,
            &ctx.accounts.lp_mint,
            &ctx.accounts.user_lp_ata,
            &ctx.accounts.token_program,
            shares as u64,
        )?;

        let position = &mut ctx.accounts.position;
        if position.owner == Pubkey::default() {
            position.owner = ctx.accounts.user.key();
            position.pool = pool_key;
            position.bump = ctx.bumps.position;
        }
        position.shares = position
            .shares
            .checked_add(shares as u64)
            .ok_or(AmmError::MathOverflow)?;
        ctx.accounts.pool.total_shares = ctx
            .accounts
            .pool
            .total_shares
            .checked_add(shares as u64)
            .ok_or(AmmError::MathOverflow)?;
        Ok(())
    }

    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        shares: u64,
        min_a: u64,
        min_b: u64,
    ) -> Result<()> {
        require!(shares > 0, AmmError::InvalidAmount);
        let pool = &ctx.accounts.pool;
        require!(
            ctx.accounts.position.shares >= shares,
            AmmError::InsufficientShares
        );
        let amount_a = (ctx.accounts.vault_a.amount as u128)
            .checked_mul(shares as u128)
            .ok_or(AmmError::MathOverflow)?
            / pool.total_shares as u128;
        let amount_b = (ctx.accounts.vault_b.amount as u128)
            .checked_mul(shares as u128)
            .ok_or(AmmError::MathOverflow)?
            / pool.total_shares as u128;
        require!(
            amount_a >= min_a as u128 && amount_b >= min_b as u128,
            AmmError::SlippageExceeded
        );

        burn_lp(
            &ctx.accounts.user_lp_ata,
            &ctx.accounts.lp_mint,
            &ctx.accounts.user,
            &ctx.accounts.token_program,
            shares,
        )?;
        let signer = pool_signer(pool);
        withdraw_tokens(
            &ctx.accounts.vault_a,
            &ctx.accounts.user_ata_a,
            &ctx.accounts.mint_a,
            &ctx.accounts.pool,
            &ctx.accounts.token_program,
            amount_a as u64,
            &signer,
        )?;
        withdraw_tokens(
            &ctx.accounts.vault_b,
            &ctx.accounts.user_ata_b,
            &ctx.accounts.mint_b,
            &ctx.accounts.pool,
            &ctx.accounts.token_program,
            amount_b as u64,
            &signer,
        )?;
        ctx.accounts.position.shares -= shares;
        ctx.accounts.pool.total_shares -= shares;
        Ok(())
    }

    pub fn swap(
        ctx: Context<Swap>,
        input_is_a: bool,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<()> {
        require!(amount_in > 0, AmmError::InvalidAmount);
        let pool = &ctx.accounts.pool;
        let (
            input_vault,
            output_vault,
            input_user,
            output_user,
            input_treasury,
            input_mint,
            output_mint,
        ) = if input_is_a {
            (
                &ctx.accounts.vault_a,
                &ctx.accounts.vault_b,
                &ctx.accounts.user_ata_a,
                &ctx.accounts.user_ata_b,
                &ctx.accounts.treasury_ata_a,
                &ctx.accounts.mint_a,
                &ctx.accounts.mint_b,
            )
        } else {
            (
                &ctx.accounts.vault_b,
                &ctx.accounts.vault_a,
                &ctx.accounts.user_ata_b,
                &ctx.accounts.user_ata_a,
                &ctx.accounts.treasury_ata_b,
                &ctx.accounts.mint_b,
                &ctx.accounts.mint_a,
            )
        };
        let fee = (amount_in as u128)
            .checked_mul(pool.fee_bps as u128)
            .ok_or(AmmError::MathOverflow)?
            / BPS;
        let effective_in = (amount_in as u128)
            .checked_sub(fee)
            .ok_or(AmmError::MathOverflow)?;
        let reserve_in = input_vault.amount as u128;
        let reserve_out = output_vault.amount as u128;
        let amount_out = reserve_out
            .checked_mul(effective_in)
            .ok_or(AmmError::MathOverflow)?
            / reserve_in
                .checked_add(effective_in)
                .ok_or(AmmError::MathOverflow)?;
        require!(
            amount_out >= min_amount_out as u128 && amount_out > 0 && amount_out < reserve_out,
            AmmError::SlippageExceeded
        );

        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: input_user.to_account_info(),
                    mint: input_mint.to_account_info(),
                    to: input_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            effective_in as u64,
            input_mint.decimals,
        )?;
        if fee > 0 {
            transfer_checked(
                CpiContext::new(
                    ctx.accounts.token_program.to_account_info(),
                    TransferChecked {
                        from: input_user.to_account_info(),
                        mint: input_mint.to_account_info(),
                        to: input_treasury.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                fee as u64,
                input_mint.decimals,
            )?;
        }
        let signer = pool_signer(pool);
        withdraw_tokens(
            output_vault,
            output_user,
            output_mint,
            &ctx.accounts.pool,
            &ctx.accounts.token_program,
            amount_out as u64,
            &signer,
        )
    }

    pub fn collect_fees(ctx: Context<CollectFees>, input_is_a: bool, amount: u64) -> Result<()> {
        require!(amount > 0, AmmError::InvalidAmount);
        require_keys_eq!(ctx.accounts.pool.treasury, ctx.accounts.treasury.key());
        let (source, destination, mint) = if input_is_a {
            (
                &ctx.accounts.treasury_ata_a,
                &ctx.accounts.treasury_destination_a,
                &ctx.accounts.mint_a,
            )
        } else {
            (
                &ctx.accounts.treasury_ata_b,
                &ctx.accounts.treasury_destination_b,
                &ctx.accounts.mint_b,
            )
        };
        require!(source.amount >= amount, AmmError::InsufficientTreasuryFees);
        transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: source.to_account_info(),
                    mint: mint.to_account_info(),
                    to: destination.to_account_info(),
                    authority: ctx.accounts.treasury.to_account_info(),
                },
            ),
            amount,
            mint.decimals,
        )
    }
}

fn deposit_tokens<'info, A: ToAccountInfo<'info>, B: ToAccountInfo<'info>>(
    authority: &Signer<'info>,
    source: &A,
    destination: &B,
    mint: &InterfaceAccount<'info, Mint>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
) -> Result<()> {
    transfer_checked(
        CpiContext::new(
            token_program.to_account_info(),
            TransferChecked {
                from: source.to_account_info(),
                mint: mint.to_account_info(),
                to: destination.to_account_info(),
                authority: authority.to_account_info(),
            },
        ),
        amount,
        mint.decimals,
    )
}

fn mint_lp<'info>(
    pool: &Account<'info, Pool>,
    mint: &InterfaceAccount<'info, Mint>,
    destination: &InterfaceAccount<'info, TokenAccount>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
) -> Result<()> {
    let signer = pool_signer(pool);
    let signer_refs: Vec<&[u8]> = signer.iter().map(Vec::as_slice).collect();
    mint_to(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            MintTo {
                mint: mint.to_account_info(),
                to: destination.to_account_info(),
                authority: pool.to_account_info(),
            },
            &[&signer_refs],
        ),
        amount,
    )
}

fn burn_lp<'info>(
    source: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    authority: &Signer<'info>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
) -> Result<()> {
    token_interface::burn(
        CpiContext::new(
            token_program.to_account_info(),
            Burn {
                mint: mint.to_account_info(),
                from: source.to_account_info(),
                authority: authority.to_account_info(),
            },
        ),
        amount,
    )
}

fn withdraw_tokens<'info>(
    source: &InterfaceAccount<'info, TokenAccount>,
    destination: &InterfaceAccount<'info, TokenAccount>,
    mint: &InterfaceAccount<'info, Mint>,
    pool: &Account<'info, Pool>,
    token_program: &Interface<'info, TokenInterface>,
    amount: u64,
    signer: &[Vec<u8>],
) -> Result<()> {
    let signer_refs: Vec<&[u8]> = signer.iter().map(Vec::as_slice).collect();
    transfer_checked(
        CpiContext::new_with_signer(
            token_program.to_account_info(),
            TransferChecked {
                from: source.to_account_info(),
                mint: mint.to_account_info(),
                to: destination.to_account_info(),
                authority: pool.to_account_info(),
            },
            &[signer_refs.as_slice()],
        ),
        amount,
        mint.decimals,
    )
}

fn pool_signer(pool: &Pool) -> [Vec<u8>; 5] {
    [
        POOL_SEED.to_vec(),
        pool.authority.to_bytes().to_vec(),
        pool.mint_a.to_bytes().to_vec(),
        pool.mint_b.to_bytes().to_vec(),
        vec![pool.bump],
    ]
}

fn integer_sqrt(value: u128) -> Result<u128> {
    let mut x = value;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + value / x) / 2;
    }
    Ok(x)
}

#[account]
pub struct Pool {
    pub authority: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub treasury: Pubkey,
    pub total_shares: u64,
    pub fee_bps: u16,
    pub bump: u8,
}

#[account]
pub struct Position {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub shares: u64,
    pub bump: u8,
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init, payer = authority, seeds = [POOL_SEED, authority.key().as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()], bump,
        space = 8 + 32 * 4 + 8 + 2 + 1
    )]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mint::token_program = token_program)]
    pub mint_a: Box<InterfaceAccount<'info, Mint>>,
    #[account(mint::token_program = token_program)]
    pub mint_b: Box<InterfaceAccount<'info, Mint>>,
    /// CHECK: The treasury is stored in the pool and used only as the authority of its ATAs.
    pub treasury: UncheckedAccount<'info>,
    #[account(init, payer = authority, seeds = [b"lp-mint", pool.key().as_ref()], bump, mint::decimals = 6, mint::authority = pool, mint::token_program = token_program)]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(init, payer = authority, seeds = [b"vault-a", pool.key().as_ref()], bump, token::mint = mint_a, token::authority = pool, token::token_program = token_program)]
    pub vault_a: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = authority, seeds = [b"vault-b", pool.key().as_ref()], bump, token::mint = mint_b, token::authority = pool, token::token_program = token_program)]
    pub vault_b: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_a, token::authority = treasury)]
    pub treasury_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_b, token::authority = treasury)]
    pub treasury_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_a, token::authority = authority)]
    pub authority_ata_a: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint_b, token::authority = authority)]
    pub authority_ata_b: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = authority, associated_token::mint = lp_mint, associated_token::authority = authority, associated_token::token_program = token_program)]
    pub authority_lp_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init, payer = authority, seeds = [POSITION_SEED, pool.key().as_ref(), authority.key().as_ref()], bump, space = 8 + 32 + 32 + 8 + 1)]
    pub position: Box<Account<'info, Position>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AddLiquidity<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, has_one = mint_a, has_one = mint_b, seeds = [POOL_SEED, pool.authority.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mint::token_program = token_program)]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(mint::token_program = token_program)]
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_a, token::authority = user)]
    pub user_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = user)]
    pub user_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_a, token::authority = pool)]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = pool)]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = lp_mint, token::authority = user)]
    pub user_lp_ata: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, seeds = [b"lp-mint", pool.key().as_ref()], bump)]
    pub lp_mint: InterfaceAccount<'info, Mint>,
    #[account(init_if_needed, payer = user, seeds = [POSITION_SEED, pool.key().as_ref(), user.key().as_ref()], bump, space = 8 + 32 + 32 + 8 + 1)]
    pub position: Account<'info, Position>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RemoveLiquidity<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, has_one = mint_a, has_one = mint_b, seeds = [POOL_SEED, pool.authority.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mint::token_program = token_program)]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(mint::token_program = token_program)]
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_a, token::authority = pool)]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = pool)]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_a, token::authority = user)]
    pub user_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = user)]
    pub user_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = lp_mint, token::authority = user)]
    pub user_lp_ata: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, seeds = [b"lp-mint", pool.key().as_ref()], bump)]
    pub lp_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, has_one = owner, seeds = [POSITION_SEED, pool.key().as_ref(), user.key().as_ref()], bump = position.bump)]
    pub position: Account<'info, Position>,
    /// CHECK: constrained to the position owner.
    pub owner: UncheckedAccount<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(has_one = mint_a, has_one = mint_b, seeds = [POOL_SEED, pool.authority.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mint::token_program = token_program)]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(mint::token_program = token_program)]
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_a, token::authority = pool)]
    pub vault_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = pool)]
    pub vault_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_a, token::authority = user)]
    pub user_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = user)]
    pub user_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_a, token::authority = pool.treasury)]
    pub treasury_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = pool.treasury)]
    pub treasury_ata_b: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct CollectFees<'info> {
    #[account(mut)]
    pub treasury: Signer<'info>,
    #[account(has_one = mint_a, has_one = mint_b, seeds = [POOL_SEED, pool.authority.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
    #[account(mint::token_program = token_program)]
    pub mint_a: InterfaceAccount<'info, Mint>,
    #[account(mint::token_program = token_program)]
    pub mint_b: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = mint_a, token::authority = treasury)]
    pub treasury_ata_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = treasury)]
    pub treasury_ata_b: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_a, token::authority = treasury)]
    pub treasury_destination_a: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = mint_b, token::authority = treasury)]
    pub treasury_destination_b: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[error_code]
pub enum AmmError {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,
    #[msg("Fee exceeds the configured maximum")]
    InvalidFee,
    #[msg("Insufficient initial liquidity")]
    InsufficientLiquidity,
    #[msg("Insufficient LP shares")]
    InsufficientShares,
    #[msg("Insufficient treasury fees")]
    InsufficientTreasuryFees,
    #[msg("Slippage limit exceeded")]
    SlippageExceeded,
    #[msg("Arithmetic overflow")]
    MathOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_sqrt_is_floor_sqrt() {
        assert_eq!(integer_sqrt(0).unwrap(), 0);
        assert_eq!(integer_sqrt(1).unwrap(), 1);
        assert_eq!(integer_sqrt(35).unwrap(), 5);
        assert_eq!(integer_sqrt(36).unwrap(), 6);
    }

    #[test]
    fn swap_quote_charges_fee_before_constant_product_quote() {
        let reserve_in = 1_000_u128;
        let reserve_out = 2_000_u128;
        let amount_in = 1_000_u128;
        let fee = amount_in * 30 / BPS;
        let effective = amount_in - fee;
        let output = reserve_out * effective / (reserve_in + effective);
        assert_eq!(fee, 3);
        assert_eq!(output, 998);
    }
}
