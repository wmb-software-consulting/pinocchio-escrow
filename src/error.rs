use pinocchio::program_error::ProgramError;

#[derive(Clone, PartialEq)]
pub enum CustomError {
    // overflow error
    WriteOverflow,
    // invalid instruction data
    InvalidInstructionData,
    // pda mismatch
    PdaMismatch,
    // Invalid Owner
    InvalidOwner,
    // Invalid Escrow
    EscrowAlreadyExists,
    // Invalid Escrow
    EscrowNotFound,
    // Invalid Seeds
    SeedsNotProvided,
}

impl From<CustomError> for ProgramError {
    fn from(e: CustomError) -> Self {
        Self::Custom(e as u32)
    }
}
