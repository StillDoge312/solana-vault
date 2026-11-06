use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::memo::Memo;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{self as token_interface, Mint as InterfaceMint, TokenAccount as InterfaceTokenAccount};
use raydium_clmm_cpi::{
    cpi,
    states::{AmmConfig, ObservationState, PersonalPositionState, PoolState, ProtocolPositionState, TickArrayState},
};

declare_id!("EbojNUfh9Jk6dyZaaQAJWofbdnkvdxfQbeAPq6iWoHAu");

#[program]
pub mod simple_vault {
    use super::*;

    /// Создать vault для конкретного SPL-токена.
    pub fn initialize_vault(ctx: Context<InitializeVault>) -> Result<()> {
        let vault = &mut ctx.accounts.vault_state;
        vault.admin = ctx.accounts.admin.key();
        vault.token_mint = ctx.accounts.token_mint.key();
        vault.vault_token_account = ctx.accounts.vault_token_account.key();
        vault.share_mint = ctx.accounts.share_mint.key();
        vault.total_tokens = 0;
        vault.total_shares = 0;
        vault.bump = ctx.bumps.vault_state;

        msg!("Vault initialized for mint: {}", vault.token_mint);
        Ok(())
    }

    /// Создать личную «карточку» пользователя в vault.
    pub fn create_user_card(ctx: Context<CreateUserCard>) -> Result<()> {
        let card = &mut ctx.accounts.user_card;
        card.vault = ctx.accounts.vault_state.key();
        card.owner = ctx.accounts.user.key();
        card.bump = ctx.bumps.user_card;

        msg!("User card created for: {}", card.owner);
        Ok(())
    }

    /// Пользователь вносит `amount` токенов → получает share-токены.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);

        let vault = &ctx.accounts.vault_state;

        let shares = if vault.total_shares == 0 || vault.total_tokens == 0 {
            amount
        } else {
            (amount as u128)
                .checked_mul(vault.total_shares as u128)
                .and_then(|v| v.checked_div(vault.total_tokens as u128))
                .and_then(|v| u64::try_from(v).ok())
                .ok_or(VaultError::MathOverflow)?
        };
        require!(shares > 0, VaultError::SharesTooSmall);

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.vault_token_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        let token_mint_key = ctx.accounts.vault_state.token_mint;
        let seeds = &[
            b"vault".as_ref(),
            token_mint_key.as_ref(),
            &[ctx.accounts.vault_state.bump],
        ];
        let signer = &[&seeds[..]];

        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    to: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.vault_state.to_account_info(),
                },
                signer,
            ),
            shares,
        )?;

        let vault = &mut ctx.accounts.vault_state;
        vault.total_tokens = vault.total_tokens.checked_add(amount).ok_or(VaultError::MathOverflow)?;
        vault.total_shares = vault.total_shares.checked_add(shares).ok_or(VaultError::MathOverflow)?;

        let card = &mut ctx.accounts.user_card;
        card.deposited = card.deposited.checked_add(amount).ok_or(VaultError::MathOverflow)?;
        card.shares = card.shares.checked_add(shares).ok_or(VaultError::MathOverflow)?;

        msg!("Deposited: {} tokens -> {} shares minted", amount, shares);
        Ok(())
    }

    /// Пользователь возвращает `shares_amount` share-токенов → получает базовые токены.
    pub fn withdraw(ctx: Context<Withdraw>, shares_amount: u64) -> Result<()> {
        require!(shares_amount > 0, VaultError::ZeroAmount);

        let vault = &ctx.accounts.vault_state;
        require!(vault.total_shares > 0, VaultError::VaultEmpty);

        let tokens_out = (shares_amount as u128)
            .checked_mul(vault.total_tokens as u128)
            .and_then(|v| v.checked_div(vault.total_shares as u128))
            .and_then(|v| u64::try_from(v).ok())
            .ok_or(VaultError::MathOverflow)?;

        require!(tokens_out > 0, VaultError::WithdrawTooSmall);

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.share_mint.to_account_info(),
                    from: ctx.accounts.user_share_account.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            shares_amount,
        )?;

        let token_mint_key = ctx.accounts.vault_state.token_mint;
        let seeds = &[
            b"vault".as_ref(),
            token_mint_key.as_ref(),
            &[ctx.accounts.vault_state.bump],
        ];
        let signer = &[&seeds[..]];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault_token_account.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.vault_state.to_account_info(),
                },
                signer,
            ),
            tokens_out,
        )?;

        let vault = &mut ctx.accounts.vault_state;
        vault.total_tokens = vault.total_tokens.checked_sub(tokens_out).ok_or(VaultError::MathOverflow)?;
        vault.total_shares = vault.total_shares.checked_sub(shares_amount).ok_or(VaultError::MathOverflow)?;

        let card = &mut ctx.accounts.user_card;
        card.deposited = card.deposited.saturating_sub(tokens_out);
        card.shares = card.shares.checked_sub(shares_amount).ok_or(VaultError::MathOverflow)?;

        msg!("Withdrawn: {} shares burned -> {} tokens returned", shares_amount, tokens_out);
        Ok(())
    }

    /// Неделя 12: Инициализация стратегии Raydium CLMM
    pub fn init_strategy(ctx: Context<InitStrategy>) -> Result<()> {
        let pool = ctx.accounts.pool_state.load()?;
        let vault_token = ctx.accounts.vault_state.token_mint;
        let second = ctx.accounts.second_mint.key();
        let pool_mints = [pool.token_mint_0, pool.token_mint_1];
        require!(
            pool_mints.contains(&vault_token) && pool_mints.contains(&second) && vault_token != second,
            VaultError::PoolMintMismatch
        );

        let strategy = &mut ctx.accounts.strategy;
        strategy.vault = ctx.accounts.vault_state.key();
        strategy.pool = ctx.accounts.pool_state.key();
        strategy.second_mint = second;
        strategy.second_token_account = ctx.accounts.second_token_account.key();
        strategy.bump = ctx.bumps.strategy;

        msg!("Strategy initialized: pool={}, second_mint={}", strategy.pool, second);
        Ok(())
    }

    /// Неделя 12: Swap через Raydium CLMM
    pub fn swap_raydium<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapRaydium<'info>>,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<()> {
        require!(amount_in > 0, VaultError::ZeroAmount);

        let token_mint_key = ctx.accounts.vault_state.token_mint;
        let vault_seeds: &[&[&[u8]]] = &[&[
            b"vault".as_ref(),
            token_mint_key.as_ref(),
            &[ctx.accounts.vault_state.bump],
        ]];

        let remaining = ctx.remaining_accounts.to_vec();

        let cpi_accounts = cpi::accounts::SwapSingleV2 {
            payer: ctx.accounts.admin.to_account_info(),
            amm_config: ctx.accounts.amm_config.to_account_info(),
            pool_state: ctx.accounts.pool_state.to_account_info(),
            input_token_account: ctx.accounts.vault_input_token_account.to_account_info(),
            output_token_account: ctx.accounts.vault_output_token_account.to_account_info(),
            input_vault: ctx.accounts.input_vault.to_account_info(),
            output_vault: ctx.accounts.output_vault.to_account_info(),
            observation_state: ctx.accounts.observation_state.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
            memo_program: ctx.accounts.memo_program.to_account_info(),
            input_vault_mint: ctx.accounts.input_vault_mint.to_account_info(),
            output_vault_mint: ctx.accounts.output_vault_mint.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.clmm_program.to_account_info(),
            cpi_accounts,
            vault_seeds,
        )
        .with_remaining_accounts(remaining);

        cpi::swap_v2(cpi_ctx, amount_in, min_amount_out, 0, true)?;

        if ctx.accounts.vault_input_token_account.mint == token_mint_key {
            let vault = &mut ctx.accounts.vault_state;
            vault.total_tokens = vault.total_tokens.saturating_sub(amount_in);
        }

        msg!("Swap executed: amount_in={}, min_amount_out={}", amount_in, min_amount_out);
        Ok(())
    }

    /// Неделя 13: Открытие позиции ликвидности в Raydium CLMM.
    /// Position NFT чеканится в пользу vault PDA (программа владеет позицией).
    pub fn open_raydium_position<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, OpenRaydiumPosition<'info>>,
        tick_lower: i32,
        tick_upper: i32,
        tick_array_lower_start: i32,
        tick_array_upper_start: i32,
        amount_0_max: u64,
        amount_1_max: u64,
    ) -> Result<()> {
        let a = &ctx.accounts;
        require_vault_storages(&a.vault_state, &a.strategy, a.token_account_0.key(), a.token_account_1.key())?;

        let open = cpi::accounts::OpenPositionWithToken22Nft {
            payer: a.admin.to_account_info(),
            position_nft_owner: a.vault_state.to_account_info(),
            position_nft_mint: a.position_nft_mint.to_account_info(),
            position_nft_account: a.position_nft_account.to_account_info(),
            pool_state: a.pool_state.to_account_info(),
            protocol_position: a.protocol_position.to_account_info(),
            tick_array_lower: a.tick_array_lower.to_account_info(),
            tick_array_upper: a.tick_array_upper.to_account_info(),
            personal_position: a.personal_position.to_account_info(),
            token_account_0: a.token_account_0.to_account_info(),
            token_account_1: a.token_account_1.to_account_info(),
            token_vault_0: a.token_vault_0.to_account_info(),
            token_vault_1: a.token_vault_1.to_account_info(),
            rent: a.rent.to_account_info(),
            system_program: a.system_program.to_account_info(),
            token_program: a.token_program.to_account_info(),
            associated_token_program: a.associated_token_program.to_account_info(),
            token_program_2022: a.token_program_2022.to_account_info(),
            vault_0_mint: a.vault_0_mint.to_account_info(),
            vault_1_mint: a.vault_1_mint.to_account_info(),
        };

        cpi::open_position_with_token22_nft(
            CpiContext::new(a.clmm_program.to_account_info(), open).with_remaining_accounts(ctx.remaining_accounts.to_vec()),
            tick_lower,
            tick_upper,
            tick_array_lower_start,
            tick_array_upper_start,
            0,
            0,
            0,
            false,
            None,
        )?;

        let token_mint = a.vault_state.token_mint;
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault".as_ref(), token_mint.as_ref(), &[a.vault_state.bump]]];

        let increase = cpi::accounts::IncreaseLiquidityV2 {
            nft_owner: a.vault_state.to_account_info(),
            nft_account: a.position_nft_account.to_account_info(),
            pool_state: a.pool_state.to_account_info(),
            protocol_position: a.protocol_position.to_account_info(),
            personal_position: a.personal_position.to_account_info(),
            tick_array_lower: a.tick_array_lower.to_account_info(),
            tick_array_upper: a.tick_array_upper.to_account_info(),
            token_account_0: a.token_account_0.to_account_info(),
            token_account_1: a.token_account_1.to_account_info(),
            token_vault_0: a.token_vault_0.to_account_info(),
            token_vault_1: a.token_vault_1.to_account_info(),
            token_program: a.token_program.to_account_info(),
            token_program_2022: a.token_program_2022.to_account_info(),
            vault_0_mint: a.vault_0_mint.to_account_info(),
            vault_1_mint: a.vault_1_mint.to_account_info(),
        };

        cpi::increase_liquidity_v2(
            CpiContext::new_with_signer(a.clmm_program.to_account_info(), increase, signer_seeds)
                .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
            0,
            amount_0_max,
            amount_1_max,
            Some(true),
        )?;

        msg!("Position opened: ticks {}..{}, nft={}", tick_lower, tick_upper, a.position_nft_mint.key());
        Ok(())
    }

    /// Неделя 13: Сбор накопленных комиссий (harvest fees) в хранилища vault
    pub fn collect_fees<'info>(ctx: Context<'_, '_, 'info, 'info, ManageVaultPosition<'info>>) -> Result<()> {
        decrease_position(&ctx, 0, 0, 0)?;
        msg!("Fees collected for nft {}", ctx.accounts.position_nft_mint.key());
        Ok(())
    }

    /// Неделя 13: Закрытие позиции, полный вывод ликвидности и сжигание Position NFT
    pub fn close_raydium_position<'info>(
        ctx: Context<'_, '_, 'info, 'info, ManageVaultPosition<'info>>,
        amount_0_min: u64,
        amount_1_min: u64,
    ) -> Result<()> {
        let liquidity = ctx.accounts.personal_position.load()?.liquidity;
        decrease_position(&ctx, liquidity, amount_0_min, amount_1_min)?;

        let a = &ctx.accounts;
        let token_mint = a.vault_state.token_mint;
        let signer_seeds: &[&[&[u8]]] = &[&[b"vault".as_ref(), token_mint.as_ref(), &[a.vault_state.bump]]];

        let close = cpi::accounts::ClosePosition {
            nft_owner: a.vault_state.to_account_info(),
            position_nft_mint: a.position_nft_mint.to_account_info(),
            position_nft_account: a.position_nft_account.to_account_info(),
            personal_position: a.personal_position.to_account_info(),
            system_program: a.system_program.to_account_info(),
            token_program: a.token_program_2022.to_account_info(),
        };

        cpi::close_position(CpiContext::new_with_signer(
            a.clmm_program.to_account_info(),
            close,
            signer_seeds,
        ))?;

        msg!("Position closed: liquidity {} returned to vault", liquidity);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions
// ─────────────────────────────────────────────────────────────────────────────

fn require_vault_storages(vault: &VaultState, strategy: &Strategy, account_0: Pubkey, account_1: Pubkey) -> Result<()> {
    let storages = [vault.vault_token_account, strategy.second_token_account];
    require!(
        storages.contains(&account_0) && storages.contains(&account_1) && account_0 != account_1,
        VaultError::NotVaultStorage
    );
    Ok(())
}

fn decrease_position<'info>(
    ctx: &Context<'_, '_, 'info, 'info, ManageVaultPosition<'info>>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
) -> Result<()> {
    let a = &ctx.accounts;
    require_vault_storages(&a.vault_state, &a.strategy, a.token_account_0.key(), a.token_account_1.key())?;

    let token_mint = a.vault_state.token_mint;
    let signer_seeds: &[&[&[u8]]] = &[&[b"vault".as_ref(), token_mint.as_ref(), &[a.vault_state.bump]]];

    let decrease = cpi::accounts::DecreaseLiquidityV2 {
        nft_owner: a.vault_state.to_account_info(),
        nft_account: a.position_nft_account.to_account_info(),
        personal_position: a.personal_position.to_account_info(),
        pool_state: a.pool_state.to_account_info(),
        protocol_position: a.protocol_position.to_account_info(),
        token_vault_0: a.token_vault_0.to_account_info(),
        token_vault_1: a.token_vault_1.to_account_info(),
        tick_array_lower: a.tick_array_lower.to_account_info(),
        tick_array_upper: a.tick_array_upper.to_account_info(),
        recipient_token_account_0: a.token_account_0.to_account_info(),
        recipient_token_account_1: a.token_account_1.to_account_info(),
        token_program: a.token_program.to_account_info(),
        token_program_2022: a.token_program_2022.to_account_info(),
        memo_program: a.memo_program.to_account_info(),
        vault_0_mint: a.vault_0_mint.to_account_info(),
        vault_1_mint: a.vault_1_mint.to_account_info(),
    };

    cpi::decrease_liquidity_v2(
        CpiContext::new_with_signer(a.clmm_program.to_account_info(), decrease, signer_seeds)
            .with_remaining_accounts(ctx.remaining_accounts.to_vec()),
        liquidity,
        amount_0_min,
        amount_1_min,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Accounts
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(
        init,
        payer = admin,
        space = VaultState::LEN,
        seeds = [b"vault", token_mint.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        init,
        payer = admin,
        token::mint = token_mint,
        token::authority = vault_state,
        seeds = [b"vault_tokens", token_mint.key().as_ref()],
        bump,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = vault_state,
        seeds = [b"share_mint", token_mint.key().as_ref()],
        bump,
    )]
    pub share_mint: Account<'info, Mint>,

    pub token_mint: Account<'info, Mint>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CreateUserCard<'info> {
    #[account(
        init,
        payer = user,
        space = UserCard::LEN,
        seeds = [b"user_card", vault_state.key().as_ref(), user.key().as_ref()],
        bump,
    )]
    pub user_card: Account<'info, UserCard>,

    pub vault_state: Account<'info, VaultState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"user_card", vault_state.key().as_ref(), user.key().as_ref()],
        bump = user_card.bump,
        has_one = vault,
        has_one = owner,
    )]
    pub user_card: Account<'info, UserCard>,

    #[account(
        mut,
        seeds = [b"vault_tokens", vault_state.token_mint.as_ref()],
        bump,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"share_mint", vault_state.token_mint.as_ref()],
        bump,
    )]
    pub share_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = vault_state.token_mint,
        token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = vault_state.share_mint,
        token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    pub vault: Account<'info, VaultState>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"user_card", vault_state.key().as_ref(), user.key().as_ref()],
        bump = user_card.bump,
        has_one = vault,
        has_one = owner,
    )]
    pub user_card: Account<'info, UserCard>,

    pub vault: Account<'info, VaultState>,
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault_tokens", vault_state.token_mint.as_ref()],
        bump,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"share_mint", vault_state.token_mint.as_ref()],
        bump,
    )]
    pub share_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = vault_state.token_mint,
        token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = vault_state.share_mint,
        token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct InitStrategy<'info> {
    #[account(
        init,
        payer = admin,
        space = Strategy::LEN,
        seeds = [b"strategy", vault_state.key().as_ref()],
        bump,
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(
        has_one = admin,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    pub second_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        token::mint = second_mint,
        token::authority = vault_state,
        seeds = [b"vault_tokens_2", vault_state.key().as_ref()],
        bump,
    )]
    pub second_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SwapRaydium<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
        constraint = vault_state.admin == admin.key() @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        seeds = [b"strategy", vault_state.key().as_ref()],
        bump = strategy.bump,
        constraint = strategy.pool == pool_state.key() @ VaultError::WrongPool,
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(
        mut,
        token::authority = vault_state,
    )]
    pub vault_input_token_account: InterfaceAccount<'info, InterfaceTokenAccount>,

    #[account(
        mut,
        token::authority = vault_state,
    )]
    pub vault_output_token_account: InterfaceAccount<'info, InterfaceTokenAccount>,

    pub amm_config: Box<Account<'info, AmmConfig>>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub observation_state: AccountLoader<'info, ObservationState>,

    pub input_vault_mint: Box<InterfaceAccount<'info, InterfaceMint>>,
    pub output_vault_mint: Box<InterfaceAccount<'info, InterfaceMint>>,

    /// CHECK: Raydium CLMM program
    #[account(address = raydium_clmm_cpi::id())]
    pub clmm_program: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    pub memo_program: Program<'info, Memo>,
}

#[derive(Accounts)]
pub struct OpenRaydiumPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
        constraint = vault_state.admin == admin.key() @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        seeds = [b"strategy", vault_state.key().as_ref()],
        bump = strategy.bump,
        constraint = strategy.pool == pool_state.key() @ VaultError::WrongPool,
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(mut)]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    /// Position NFT Mint
    #[account(mut)]
    pub position_nft_mint: Signer<'info>,

    /// Position NFT ATA — принадлежит vault PDA
    #[account(
        init,
        payer = admin,
        associated_token::mint = position_nft_mint,
        associated_token::authority = vault_state,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    #[account(mut)]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    #[account(mut)]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    #[account(mut)]
    pub token_account_0: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_account_1: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_vault_0: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_vault_1: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program_2022: Program<'info, Token2022>,

    pub vault_0_mint: Box<InterfaceAccount<'info, InterfaceMint>>,
    pub vault_1_mint: Box<InterfaceAccount<'info, InterfaceMint>>,

    /// CHECK: Raydium CLMM program
    #[account(address = raydium_clmm_cpi::id())]
    pub clmm_program: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ManageVaultPosition<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
        constraint = vault_state.admin == admin.key() @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        seeds = [b"strategy", vault_state.key().as_ref()],
        bump = strategy.bump,
        constraint = strategy.pool == pool_state.key() @ VaultError::WrongPool,
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    #[account(mut)]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    #[account(mut)]
    pub position_nft_mint: Box<InterfaceAccount<'info, InterfaceMint>>,

    #[account(
        mut,
        token::mint = position_nft_mint,
        token::authority = vault_state,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub personal_position: AccountLoader<'info, PersonalPositionState>,

    #[account(mut)]
    pub tick_array_lower: AccountLoader<'info, TickArrayState>,

    #[account(mut)]
    pub tick_array_upper: AccountLoader<'info, TickArrayState>,

    #[account(mut)]
    pub token_account_0: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_account_1: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_vault_0: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    #[account(mut)]
    pub token_vault_1: Box<InterfaceAccount<'info, InterfaceTokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    pub memo_program: Program<'info, Memo>,

    pub vault_0_mint: Box<InterfaceAccount<'info, InterfaceMint>>,
    pub vault_1_mint: Box<InterfaceAccount<'info, InterfaceMint>>,

    pub system_program: Program<'info, System>,

    /// CHECK: Raydium CLMM program
    #[account(address = raydium_clmm_cpi::id())]
    pub clmm_program: UncheckedAccount<'info>,
}

// ─────────────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────────────

#[account]
#[derive(Default)]
pub struct VaultState {
    pub admin: Pubkey,               // 32
    pub token_mint: Pubkey,          // 32 — SPL-токен vault'а
    pub vault_token_account: Pubkey, // 32 — PDA для токенов
    pub share_mint: Pubkey,          // 32 — mint для shares
    pub total_tokens: u64,           // 8  — всего токенов в vault
    pub total_shares: u64,           // 8  — всего выпущено shares
    pub bump: u8,                    // 1
}

impl VaultState {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 8 + 8 + 1;
}

#[account]
#[derive(Default)]
pub struct UserCard {
    pub vault: Pubkey,  // 32 — к какому vault относится
    pub owner: Pubkey,  // 32 — владелец карточки
    pub deposited: u64, // 8 — сколько токенов внёс пользователь
    pub shares: u64,    // 8 — сколько shares у пользователя
    pub bump: u8,       // 1
}

impl UserCard {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 1;
}

#[account]
#[derive(Default)]
pub struct Strategy {
    pub vault: Pubkey,                // 32 — vault PDA
    pub pool: Pubkey,                 // 32 — пул Raydium CLMM
    pub second_mint: Pubkey,          // 32 — второй токен пула
    pub second_token_account: Pubkey, // 32 — второе хранилище (PDA)
    pub bump: u8,                     // 1
}

impl Strategy {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 1;
}

// ─────────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────────

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Shares would be zero - deposit too small")]
    SharesTooSmall,
    #[msg("Vault is empty")]
    VaultEmpty,
    #[msg("Withdraw amount too small")]
    WithdrawTooSmall,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Unauthorized access")]
    Unauthorized,
    #[msg("Pool mint mismatch")]
    PoolMintMismatch,
    #[msg("Swaps are allowed only through the strategy pool")]
    WrongPool,
    #[msg("Account is not a registered vault storage")]
    NotVaultStorage,
}
