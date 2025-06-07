use pinocchio::{
    account_info::AccountInfo,
    instruction::{ Seed, Signer },
    program_error::ProgramError,
    sysvars::{ rent::Rent, Sysvar },
    ProgramResult,
};

use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::state::Mint;
use pinocchio_log::log;

use crate::{ DataLen, Escrow, CustomError };

use super::{ validate_ata, validate_mint, validate_acc, validate_programs };

pub struct MakeAccounts<'a> {
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub mint_b: &'a AccountInfo,
    pub maker_ata_a: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
    pub bumps: [u8; 2],
}

impl<'a> TryFrom<(u64, &'a [AccountInfo])> for MakeAccounts<'a> {
    type Error = ProgramError;

    fn try_from((seed, accounts): (u64, &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let [
            maker,
            escrow,
            vault,
            maker_ata_a,
            mint_a,
            mint_b,
            ata_program,
            token_program,
            system_program,
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !maker.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let (_, escrow_bump) = validate_acc::<Escrow>(
            escrow,
            false,
            Some(&[Escrow::SEED.as_bytes(), maker.key(), seed.to_le_bytes().as_ref()])
        )?;

        let (_, vault_bump) = validate_ata(vault, mint_a.key(), escrow.key(), Some(false))?;

        if mint_a.key().eq(mint_b.key()) {
            return Err(CustomError::InvalidInstructionData.into());
        }

        if let Some(value) = validate_mint(mint_a) {
            return Err(value);
        }
        if let Some(value) = validate_mint(mint_b) {
            return Err(value);
        }

        validate_ata(maker_ata_a, mint_a.key(), maker.key(), Some(true))?;

        if let Some(value) = validate_programs(ata_program, token_program, system_program) {
            return Err(value);
        }

        Ok(Self {
            maker,
            escrow,
            vault,
            maker_ata_a,
            mint_a,
            mint_b,
            system_program,
            token_program,
            bumps: [escrow_bump, vault_bump],
        })
    }
}

pub struct MakeInstructionData {
    pub seed: u64,
    pub amount: u64,
    pub receive: u64,
}

impl DataLen for MakeInstructionData {
    const LEN: usize = core::mem::size_of::<MakeInstructionData>();
}

impl<'a> TryFrom<&'a [u8]> for MakeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if data.len() != MakeInstructionData::LEN {
            return Err(ProgramError::InvalidInstructionData);
        }

        let seed = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let amount = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let receive = u64::from_le_bytes(data[16..24].try_into().unwrap());

        if seed.eq(&0) {
            return Err(ProgramError::InvalidInstructionData);
        }
        if amount.eq(&0) {
            return Err(ProgramError::InvalidInstructionData);
        }
        if receive.eq(&0) {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self { seed, amount, receive })
    }
}

pub struct Make<'a> {
    pub accounts: MakeAccounts<'a>,
    pub instruction_datas: MakeInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Make<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let instruction_datas: MakeInstructionData = MakeInstructionData::try_from(data)?;
        let accounts = MakeAccounts::try_from((instruction_datas.seed, accounts))?;

        Ok(Self {
            accounts,
            instruction_datas,
        })
    }
}

impl<'a> Make<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        Escrow::validate_pda(
            self.accounts.bumps[0],
            self.accounts.escrow.key(),
            &self.accounts.maker.key(),
            self.instruction_datas.seed
        )?;

        self.create_escrow_account()?;

        self.create_vault_ata()?;

        self.transfer_checked()?;

        Escrow::initialize(self.accounts.escrow, self)?;

        log!(
            "ESCROW_EVENT:INITIALIZED {} {} {} {} {}",
            self.accounts.escrow.key(),
            self.accounts.maker.key(),
            self.instruction_datas.amount,
            self.accounts.mint_a.key(),
            self.instruction_datas.receive
        );
        Ok(())
    }

    fn create_escrow_account(&self) -> ProgramResult {
        let binding = self.instruction_datas.seed.to_le_bytes();
        let pda_bump_bytes = [self.accounts.bumps[0]];
        let signer_seeds = [
            Seed::from(Escrow::SEED.as_bytes()),
            Seed::from(self.accounts.maker.key()),
            Seed::from(binding.as_ref()),
            Seed::from(&pda_bump_bytes[..]),
        ];

        let rent = Rent::get()?;
        let lamports = rent.minimum_balance(Escrow::LEN);
        (CreateAccount {
            from: self.accounts.maker,
            to: self.accounts.escrow,
            space: Escrow::LEN as u64,
            owner: &crate::ID,
            lamports: lamports,
        }).invoke_signed(&[Signer::from(&signer_seeds[..])])
    }

    fn create_vault_ata(&self) -> ProgramResult {
        (pinocchio_associated_token_account::instructions::Create {
            funding_account: self.accounts.maker,
            account: self.accounts.vault,
            wallet: self.accounts.escrow,
            mint: self.accounts.mint_a,
            system_program: self.accounts.system_program,
            token_program: self.accounts.token_program,
        }).invoke()
    }

    fn transfer_checked(&self) -> ProgramResult {
        let mint_a = Mint::from_account_info(self.accounts.mint_a)?;
        (pinocchio_token::instructions::TransferChecked {
            from: self.accounts.maker_ata_a,
            mint: self.accounts.mint_a,
            decimals: mint_a.decimals(),
            to: self.accounts.vault,
            authority: self.accounts.maker,
            amount: self.instruction_datas.amount,
        }).invoke()
    }
}
