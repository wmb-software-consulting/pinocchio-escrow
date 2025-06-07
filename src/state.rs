use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::{ self, Pubkey },
    ProgramResult,
};

use crate::{ error::CustomError, load_acc_mut_unchecked, DataLen, Initialized, Make };

#[repr(C)] //keeps the struct layout the same across different architectures
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Escrow {
    /// Indicates whether the escrow has been initialized
    pub is_initialized: bool,
    /// Unique seed for the escrow, used to derive the PDA
    pub seed: u64,
    /// The maker of the escrow, who initiates the transaction
    pub maker: Pubkey,
    /// The mint address of token A involved in the escrow
    pub mint_a: Pubkey,
    /// The mint address of token B involved in the escrow
    pub mint_b: Pubkey,
    /// The amount of token B to be received in the escrow
    pub receive: u64,
    /// The bump seed used to derive the PDA for the escrow
    pub bump: u8,
}

impl DataLen for Escrow {
    const LEN: usize = core::mem::size_of::<Escrow>();
}

impl Initialized for Escrow {
    fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    fn bump(&self) -> u8 {
        self.bump
    }
}

impl Escrow {
    pub const SEED: &'static str = "escrow";

    pub fn validate_pda(
        bump: u8,
        pda: &Pubkey,
        owner: &Pubkey,
        seed: u64
    ) -> Result<(), ProgramError> {
        let binding = seed.to_le_bytes();
        let seed_with_bump = &[Self::SEED.as_bytes(), owner, binding.as_ref(), &[bump]];
        let derived = pubkey::create_program_address(seed_with_bump, &crate::ID)?;
        if derived != *pda {
            return Err(CustomError::PdaMismatch.into());
        }
        Ok(())
    }

    pub fn initialize(acc_info: &AccountInfo, ix_data: &Make) -> ProgramResult {
        let escrow = (unsafe {
            load_acc_mut_unchecked::<Escrow>(acc_info.borrow_mut_data_unchecked())
        })?;

        escrow.maker = *ix_data.accounts.maker.key();
        escrow.mint_a = *ix_data.accounts.mint_a.key();
        escrow.mint_b = *ix_data.accounts.mint_b.key();
        escrow.seed = ix_data.instruction_datas.seed;
        escrow.is_initialized = true;
        escrow.receive = ix_data.instruction_datas.receive;
        escrow.bump = ix_data.accounts.bumps[0];

        escrow.invariant()?;
        Ok(())
    }

    pub fn invariant(self: &Escrow) -> Result<(), ProgramError> {
        if !self.is_initialized {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if self.maker == self.mint_a || self.maker == self.mint_b {
            return Err(CustomError::InvalidOwner.into());
        }
        if self.maker == pubkey::Pubkey::default() {
            return Err(CustomError::InvalidOwner.into());
        }
        if self.mint_a == self.mint_b {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if self.mint_a == pubkey::Pubkey::default() || self.mint_b == pubkey::Pubkey::default() {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if self.receive == 0 {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if self.seed == 0 {
            return Err(CustomError::InvalidInstructionData.into());
        }
        if self.bump == 0 {
            return Err(CustomError::InvalidInstructionData.into());
        }
        Ok(())
    }
}
