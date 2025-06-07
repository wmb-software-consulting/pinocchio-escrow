use pinocchio::{
    account_info::{ AccountInfo, Ref },
    program_error::ProgramError,
    pubkey::{ find_program_address, Pubkey },
};

use pinocchio_token::state::{ Mint, TokenAccount };

use crate::{ load_acc_unchecked, CustomError, DataLen, Initialized };

pub fn validate_acc<'a, T: DataLen + Initialized>(
    acc: &'a AccountInfo,
    is_initialized: bool,
    seeds: Option<&[&[u8]]>
) -> Result<(Option<&'a T>, u8), ProgramError> {
    if !acc.is_owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    if !is_initialized && (acc.lamports().ne(&0) || !acc.data_is_empty()) {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    if is_initialized && (acc.lamports().eq(&0) || acc.data_is_empty()) {
        return Err(ProgramError::UninitializedAccount);
    }
    if is_initialized {
        let loaded_acc = (unsafe {
            load_acc_unchecked::<T>(acc.borrow_data_unchecked()).map_err(
                |_| CustomError::InvalidInstructionData
            )
        })?;
        Ok((Some(loaded_acc), loaded_acc.bump()))
    } else if let Some(seeds) = seeds {
        let (key, bump) = find_program_address(seeds, &crate::ID);
        if acc.key().ne(&key) {
            return Err(ProgramError::InvalidArgument);
        }
        Ok((None, bump))
    } else {
        return Err(CustomError::SeedsNotProvided.into());
    }
}

pub fn validate_ata<'a>(
    ata: &'a AccountInfo,
    mint: &'a Pubkey,
    owner: &'a Pubkey,
    is_initialized: Option<bool>
) -> Result<(Option<Ref<'a, TokenAccount>>, u8), ProgramError> {
    let (key, bump) = find_program_address(
        &[owner, &pinocchio_token::ID.as_ref(), mint],
        &pinocchio_associated_token_account::ID
    );
    if ata.key().ne(&key) {
        return Err(ProgramError::InvalidArgument);
    }
    if is_initialized.is_none() {
        Ok((None, bump))
    } else if is_initialized == Some(false) {
        if ata.lamports().ne(&0) {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        if !ata.data_is_empty() {
            return Err(ProgramError::AccountAlreadyInitialized);
        }
        if !ata.is_owned_by(&pinocchio_system::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        Ok((None, bump))
    } else if let Ok(ata_data) = TokenAccount::from_account_info(ata) {
        if ata.lamports().eq(&0) {
            return Err(ProgramError::InvalidAccountData);
        }
        if ata_data.mint().ne(mint) {
            return Err(ProgramError::InvalidAccountData);
        }
        if ata_data.owner().ne(owner) {
            return Err(CustomError::InvalidOwner.into());
        }
        if is_initialized == Some(true) && !ata_data.is_initialized() {
            return Err(ProgramError::UninitializedAccount);
        }
        Ok((Some(ata_data), bump))
    } else {
        Err(ProgramError::InvalidAccountData)
    }
}

pub fn validate_mint(mint: &AccountInfo) -> Option<ProgramError> {
    let mint_data = Mint::from_account_info(mint)
        .map_err(|_| ProgramError::InvalidAccountData)
        .ok()?;
    if !mint_data.is_initialized() {
        return Some(ProgramError::UninitializedAccount);
    }
    if mint_data.decimals() > 9 {
        return Some(CustomError::InvalidInstructionData.into());
    }
    if mint_data.supply() == 0 {
        return Some(CustomError::InvalidInstructionData.into());
    }
    None
}

pub fn validate_programs<'a>(
    ata_program: &AccountInfo,
    token_program: &AccountInfo,
    system_program: &AccountInfo
) -> Option<ProgramError> {
    if ata_program.key() != &pinocchio_associated_token_account::ID {
        return Some(ProgramError::InvalidArgument);
    }
    if token_program.key() != &pinocchio_token::ID {
        return Some(ProgramError::InvalidArgument);
    }
    if system_program.key() != &pinocchio_system::ID {
        return Some(ProgramError::InvalidArgument);
    }
    None
}
