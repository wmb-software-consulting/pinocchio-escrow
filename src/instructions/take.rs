use pinocchio::{
    account_info::AccountInfo,
    instruction::{ Seed, Signer },
    msg,
    program_error::ProgramError,
    ProgramResult,
};

use pinocchio_token::state::{ Mint, TokenAccount };
use pinocchio_log::log;

use crate::{ load_acc_unchecked, CustomError, Escrow };

use super::{ validate_ata, validate_mint, validate_acc, validate_programs };

pub struct TakeAccounts<'a> {
    pub taker: &'a AccountInfo,
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub taker_ata_a: &'a AccountInfo,
    pub taker_ata_b: &'a AccountInfo,
    pub maker_ata_b: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub bumps: [u8; 5],
}

impl<'a> TryFrom<&'a [AccountInfo]> for TakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [
            taker,
            maker,
            escrow,
            mint_a,
            mint_b,
            vault,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            ata_program,
            token_program,
            system_program,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !taker.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let (escrow_acc, escrow_bump) = validate_acc::<Escrow>(escrow, true, None)?;
        if Some(escrow_acc).is_none() {
            return Err(CustomError::InvalidInstructionData.into());
        }

        let escrow_acc = escrow_acc.unwrap();
        if escrow_acc.maker.as_slice() != maker.key() {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if escrow_acc.mint_a.as_slice() != mint_a.key() {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if escrow_acc.mint_b.as_slice() != mint_b.key() {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if escrow_acc.receive == 0 {
            return Err(CustomError::InvalidInstructionData.into());
        }

        let (vault_acc, vault_bump) = validate_ata(vault, mint_a.key(), escrow.key(), Some(true))?;
        if vault_acc.is_none() {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if mint_a.key().eq(mint_b.key()) {
            return Err(CustomError::InvalidInstructionData.into());
        }

        if let Some(value) = validate_mint(mint_a) {
            return Err(value);
        }
        if let Some(value) = validate_mint(mint_b) {
            return Err(value);
        }

        let check_taker_ata_a = if !taker_ata_a.data_is_empty() && taker_ata_a.lamports().ne(&0) {
            Some(true)
        } else {
            None
        };

        let (_, taker_ata_a_bump) = validate_ata(
            taker_ata_a,
            mint_a.key(),
            taker.key(),
            check_taker_ata_a
        )?;
        let (_, taker_ata_b_bump) = validate_ata(
            taker_ata_b,
            mint_b.key(),
            taker.key(),
            Some(true)
        )?;

        let check_maker_ata_b = if !maker_ata_b.data_is_empty() && maker_ata_b.lamports().ne(&0) {
            Some(true)
        } else {
            None
        };

        let (_, maker_ata_b_bump) = validate_ata(
            maker_ata_b,
            mint_b.key(),
            maker.key(),
            check_maker_ata_b
        )?;

        if let Some(value) = validate_programs(ata_program, token_program, system_program) {
            return Err(value);
        }

        Ok(Self {
            taker,
            maker,
            escrow,
            vault,
            mint_a,
            mint_b,
            taker_ata_a,
            taker_ata_b,
            maker_ata_b,
            system_program,
            token_program,
            bumps: [escrow_bump, vault_bump, taker_ata_a_bump, taker_ata_b_bump, maker_ata_b_bump],
        })
    }
}

pub struct Take<'a> {
    pub accounts: TakeAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Take<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let accounts = TakeAccounts::try_from(accounts)?;

        Ok(Self {
            accounts,
        })
    }
}

impl<'a> Take<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        self.create_if_needed_taker_ata_a()?;

        msg!("TakeAccounts: Taker ATA A Bump: {}, Taker ATA B Bump: {}");
        self.create_if_needed_maker_ata_b()?;

        let escrow_acc = (unsafe {
            load_acc_unchecked::<Escrow>(self.accounts.escrow.borrow_data_unchecked()).map_err(
                |_| CustomError::InvalidInstructionData
            )
        })?;

        self.transfer_to_maker(escrow_acc)?;

        self.withdraw_and_close_vault(escrow_acc)?;

        log!(
            "Take: {} tokens transferred {} to maker {}",
            self.accounts.taker.key(),
            escrow_acc.receive,
            self.accounts.maker.key()
        );
        Ok(())
    }

    fn create_if_needed_taker_ata_a(&self) -> ProgramResult {
        let taker_ata_a = TokenAccount::from_account_info(self.accounts.taker_ata_a);
        if taker_ata_a.is_ok() && taker_ata_a?.is_initialized() {
            return Ok(());
        }
        (pinocchio_associated_token_account::instructions::Create {
            funding_account: self.accounts.taker,
            account: self.accounts.taker_ata_a,
            wallet: self.accounts.taker,
            mint: self.accounts.mint_a,
            system_program: self.accounts.system_program,
            token_program: self.accounts.token_program,
        }).invoke()
    }

    fn create_if_needed_maker_ata_b(&self) -> ProgramResult {
        let maker_ata_b = TokenAccount::from_account_info(self.accounts.maker_ata_b);
        if maker_ata_b.is_ok() && maker_ata_b?.is_initialized() {
            return Ok(());
        }
        (pinocchio_associated_token_account::instructions::Create {
            funding_account: self.accounts.taker,
            account: self.accounts.maker_ata_b,
            wallet: self.accounts.maker,
            mint: self.accounts.mint_b,
            system_program: self.accounts.system_program,
            token_program: self.accounts.token_program,
        }).invoke()
    }

    fn transfer_to_maker(&mut self, escrow: &Escrow) -> ProgramResult {
        let taker_ata_b_info = self.accounts.taker_ata_b;
        let mint_b_info = self.accounts.mint_b;
        let maker_ata_b_info = self.accounts.maker_ata_b;
        let taker_info = self.accounts.taker;

        let mint_decimals = {
            // drops the strong reference to the mint account info right after reading the decimals
            // to avoid holding a strong reference to the mint account info for too long
            let acc = Mint::from_account_info(mint_b_info)?;
            acc.decimals()
        };

        // Transfer Token B (Escrow -> Maker)
        (pinocchio_token::instructions::TransferChecked {
            from: taker_ata_b_info,
            mint: mint_b_info,
            decimals: mint_decimals,
            to: maker_ata_b_info,
            authority: taker_info,
            amount: escrow.receive,
        }).invoke()
    }

    fn withdraw_and_close_vault(&mut self, escrow: &Escrow) -> ProgramResult {
        let seed_bytes = escrow.seed.to_le_bytes();
        let pda_bump_bytes = [escrow.bump];
        let signer_seeds = [
            Seed::from(Escrow::SEED.as_bytes()),
            Seed::from(self.accounts.maker.key()),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(&pda_bump_bytes[..]),
        ];

        let mint_a_info = self.accounts.mint_a;
        let vault_info = self.accounts.vault;
        let taker_ata_a_info = self.accounts.taker_ata_a;
        let escrow_info = self.accounts.escrow;
        let maker_info = self.accounts.maker;

        let mint_decimals = {
            // drops the strong reference to the mint account info right after reading the decimals
            // to avoid holding a strong reference to the mint account info for too long
            let acc = Mint::from_account_info(mint_a_info)?;
            acc.decimals()
        };
        let amount = {
            // drops the strong reference to the vault account info right after reading the amount
            // to avoid holding a strong reference to the vault account info for too long
            let acc = TokenAccount::from_account_info(vault_info)?;
            acc.amount()
        };

        // Transfer Token A (Vault -> Taker)
        (pinocchio_token::instructions::TransferChecked {
            from: vault_info,
            mint: mint_a_info,
            decimals: mint_decimals,
            to: taker_ata_a_info,
            authority: escrow_info,
            amount: amount,
        }).invoke_signed(&[Signer::from(&signer_seeds[..])])?;

        // Close the Vault
        (pinocchio_token::instructions::CloseAccount {
            account: vault_info,
            authority: escrow_info,
            destination: maker_info,
        }).invoke_signed(&[Signer::from(&signer_seeds[..])])?;
        Ok(())
    }
}
