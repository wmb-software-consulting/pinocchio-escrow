use pinocchio::{
    account_info::AccountInfo,
    instruction::{ Seed, Signer },
    program_error::ProgramError,
    ProgramResult,
};

use pinocchio_token::state::{ Mint, TokenAccount };
use pinocchio_log::log;

use crate::{ load_acc_unchecked, CustomError, Escrow };

use super::{ validate_ata, validate_acc, validate_programs };

pub struct RefundAccounts<'a> {
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub maker_ata_a: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub bumps: [u8; 3],
}

impl<'a> TryFrom<&'a [AccountInfo]> for RefundAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [
            maker,
            escrow,
            mint_a,
            vault,
            maker_ata_a,
            ata_program,
            token_program,
            system_program,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !maker.is_signer() {
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
        if escrow_acc.receive == 0 {
            return Err(CustomError::InvalidInstructionData.into());
        }

        let (vault_acc, vault_bump) = validate_ata(vault, mint_a.key(), escrow.key(), Some(true))?;
        if vault_acc.is_none() {
            return Err(CustomError::InvalidInstructionData.into());
        }

        let check_maker_ata_a = if !maker_ata_a.data_is_empty() && maker_ata_a.lamports().ne(&0) {
            Some(true)
        } else {
            None
        };

        let (_, maker_ata_a_bump) = validate_ata(
            maker_ata_a,
            mint_a.key(),
            maker.key(),
            check_maker_ata_a
        )?;

        if let Some(value) = validate_programs(ata_program, token_program, system_program) {
            return Err(value);
        }

        Ok(Self {
            maker,
            escrow,
            vault,
            mint_a,
            maker_ata_a,
            system_program,
            token_program,
            bumps: [escrow_bump, vault_bump, maker_ata_a_bump],
        })
    }
}

pub struct Refund<'a> {
    pub accounts: RefundAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Refund<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let accounts = RefundAccounts::try_from(accounts)?;

        Ok(Self {
            accounts,
        })
    }
}

impl<'a> Refund<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        self.create_if_needed_maker_ata_a()?;

        let escrow_acc = (unsafe {
            load_acc_unchecked::<Escrow>(self.accounts.escrow.borrow_data_unchecked()).map_err(
                |_| CustomError::InvalidInstructionData
            )
        })?;

        self.withdraw_and_close_vault(escrow_acc)?;

        log!("Refund: tokens transferred to maker {}", self.accounts.maker.key());
        Ok(())
    }

    fn create_if_needed_maker_ata_a(&self) -> ProgramResult {
        let maker_ata_a = TokenAccount::from_account_info(self.accounts.maker_ata_a);
        if maker_ata_a.is_ok() && maker_ata_a?.is_initialized() {
            return Ok(());
        }
        (pinocchio_associated_token_account::instructions::Create {
            funding_account: self.accounts.maker,
            account: self.accounts.maker_ata_a,
            wallet: self.accounts.maker,
            mint: self.accounts.mint_a,
            system_program: self.accounts.system_program,
            token_program: self.accounts.token_program,
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

        let maker_ata_a_info = self.accounts.maker_ata_a;
        let mint_a_info = self.accounts.mint_a;
        let vault_info = self.accounts.vault;
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
            to: maker_ata_a_info,
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
