use mollusk_svm::result::{ Check };
use mollusk_svm::{ Mollusk };
use mollusk_svm_programs_token::{ self };
use solana_sdk::account::WritableAccount;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::program_option::COption;
use solana_sdk::program_pack::Pack;
use solana_sdk::{
    pubkey,
    account::Account,
    instruction::{ AccountMeta, Instruction },
    pubkey::Pubkey,
};
use spl_associated_token_account_client::address::get_associated_token_address;
use spl_token::{ state::{ Account as SPLTokenAccount, Mint as SPLMint, AccountState } };

use pinocchio_token::state::{ TokenAccount, Mint };
use pinocchio_escrow::{ to_bytes, Escrow, Make, MakeInstructionData, Refund, Take, ID };

pub const PROGRAM: Pubkey = Pubkey::new_from_array(ID);

pub const PAYER: Pubkey = pubkey!("9vCdf2rh7hA7JdSVV1LEbJGFDNLjk1KHGTZW1wSRN6vC");

pub fn mollusk() -> Mollusk {
    let mut mollusk = Mollusk::new(&PROGRAM, "target/deploy/pinocchio_escrow");
    mollusk_svm_programs_token::token::add_program(&mut mollusk);
    mollusk_svm_programs_token::associated_token::add_program(&mut mollusk);
    // associated_token.add_program(
    //     &Pubkey::new_from_array(ATA_PROGRAM_ID),
    //     "tests/elf_files/spl_ata",
    //     &mollusk_svm::program::loader_keys::LOADER_V3
    // );
    mollusk
}

#[test]
fn test_make() {
    let mollusk = mollusk();
    let (system_program, system_account) = mollusk_svm::program::keyed_account_for_system_program();

    let (token_program, token_program_account) = mollusk_svm_programs_token::token::keyed_account();

    let (ata_program, ata_program_account) =
        mollusk_svm_programs_token::associated_token::keyed_account();

    let maker = Pubkey::new_from_array([0x01; 32]);
    let maker_account = Account::new(10 * LAMPORTS_PER_SOL, 0, &system_program);

    let mint_a = Pubkey::new_from_array([0x02; 32]);
    let mut mint_a_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_a_account.data_as_mut_slice()
    ).unwrap();

    let mint_b = Pubkey::new_from_array([0x03; 32]);
    let mut mint_b_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_b_account.data_as_mut_slice()
    ).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[Escrow::SEED.as_bytes(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );
    let escrow_account = Account::new(0, 0, &system_program);

    let vault_account = Account::new(0, 0, &system_program);
    let vault = get_associated_token_address(&escrow, &mint_a);

    let mut maker_ata_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(TokenAccount::LEN),
        TokenAccount::LEN,
        &token_program
    );
    let maker_ata = get_associated_token_address(&maker, &mint_a);

    SPLTokenAccount::pack(
        SPLTokenAccount {
            mint: mint_a,
            owner: maker,
            amount: 15 * LAMPORTS_PER_SOL,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        maker_ata_account.data_as_mut_slice()
    ).unwrap();

    // Create the instruction data
    let instruction_data = MakeInstructionData {
        seed,
        amount: 15 * LAMPORTS_PER_SOL,
        receive: 10 * LAMPORTS_PER_SOL,
    };

    // instruction discriminator = 0
    let mut ser_instruction_data = vec![*Make::DISCRIMINATOR];
    ser_instruction_data.extend_from_slice(unsafe {
        to_bytes::<MakeInstructionData>(&instruction_data)
    });

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &ser_instruction_data,
        vec![
            AccountMeta::new(maker, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(maker_ata, false),
            AccountMeta::new_readonly(mint_a, false),
            AccountMeta::new_readonly(mint_b, false),
            AccountMeta::new_readonly(ata_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program, false)
        ]
    );

    let expected_escrow = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: mint_a.to_bytes(),
        mint_b: mint_b.to_bytes(),
        receive: 10 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };
    let serialized_expected_escrow = unsafe { to_bytes::<Escrow>(&expected_escrow) };

    let expected_maker_ata = SPLTokenAccount {
        mint: mint_a,
        owner: maker,
        amount: 0 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_maker_ata = { data_to_bytes::<SPLTokenAccount>(expected_maker_ata) };

    let expected_vault_ata = SPLTokenAccount {
        mint: mint_a,
        owner: escrow,
        amount: 15 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_vault_ata = { data_to_bytes::<SPLTokenAccount>(expected_vault_ata) };

    mollusk.process_and_validate_instruction(
        &instruction,
        &vec![
            (maker, maker_account),
            (escrow, escrow_account),
            (vault, vault_account),
            (maker_ata, maker_ata_account),
            (mint_a, mint_a_account),
            (mint_b, mint_b_account),
            (ata_program, ata_program_account),
            (token_program, token_program_account),
            (system_program, system_account)
        ],
        &[
            Check::success(),
            Check::account(&escrow).data(serialized_expected_escrow).build(),
            Check::account(&escrow).owner(&PROGRAM).build(),
            Check::account(&vault).data(serialized_expected_vault_ata.as_slice()).build(),
            Check::account(&maker_ata).data(serialized_expected_maker_ata.as_slice()).build(),
        ]
    );
}

#[test]
fn test_take() {
    let mollusk = mollusk();
    let (system_program, system_account) = mollusk_svm::program::keyed_account_for_system_program();

    let (token_program, token_program_account) = mollusk_svm_programs_token::token::keyed_account();

    let (ata_program, ata_program_account) =
        mollusk_svm_programs_token::associated_token::keyed_account();

    let maker = Pubkey::new_from_array([0x01; 32]);
    let maker_account = Account::new(10 * LAMPORTS_PER_SOL, 0, &system_program);

    let taker = Pubkey::new_from_array([0x11; 32]);
    let taker_account = Account::new(10 * LAMPORTS_PER_SOL, 0, &system_program);

    let mint_a = Pubkey::new_from_array([0x02; 32]);
    let mut mint_a_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_a_account.data_as_mut_slice()
    ).unwrap();

    let mint_b = Pubkey::new_from_array([0x03; 32]);
    let mut mint_b_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_b_account.data_as_mut_slice()
    ).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[Escrow::SEED.as_bytes(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );
    let escrow_account = Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: (
            unsafe {
                to_bytes::<Escrow>(
                    &(Escrow {
                        is_initialized: true,
                        seed,
                        maker: maker.to_bytes(),
                        mint_a: mint_a.to_bytes(),
                        mint_b: mint_b.to_bytes(),
                        receive: 10 * LAMPORTS_PER_SOL,
                        bump: escrow_bump,
                    })
                )
            }
        ).to_vec(),
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    };

    let mut vault_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(TokenAccount::LEN),
        TokenAccount::LEN,
        &token_program
    );
    let vault = get_associated_token_address(&escrow, &mint_a);

    SPLTokenAccount::pack(
        SPLTokenAccount {
            mint: mint_a,
            owner: escrow,
            amount: 15 * LAMPORTS_PER_SOL,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        vault_account.data_as_mut_slice()
    ).unwrap();

    let taker_ata_a_account = Account::new(0, 0, &system_program);
    let taker_ata_a = get_associated_token_address(&taker, &mint_a);

    let mut taker_ata_b_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(TokenAccount::LEN),
        TokenAccount::LEN,
        &token_program
    );
    let taker_ata_b = get_associated_token_address(&taker, &mint_b);

    SPLTokenAccount::pack(
        SPLTokenAccount {
            mint: mint_b,
            owner: taker,
            amount: 15 * LAMPORTS_PER_SOL,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        taker_ata_b_account.data_as_mut_slice()
    ).unwrap();

    let mut maker_ata_b_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(TokenAccount::LEN),
        TokenAccount::LEN,
        &token_program
    );
    let maker_ata_b = get_associated_token_address(&maker, &mint_b);
    SPLTokenAccount::pack(
        SPLTokenAccount {
            mint: mint_b,
            owner: maker,
            amount: 0 * LAMPORTS_PER_SOL,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        maker_ata_b_account.data_as_mut_slice()
    ).unwrap();

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &vec![*Take::DISCRIMINATOR],
        vec![
            AccountMeta::new(taker, true),
            AccountMeta::new(maker, false),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(mint_a, false),
            AccountMeta::new_readonly(mint_b, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(taker_ata_a, false),
            AccountMeta::new(taker_ata_b, false),
            AccountMeta::new(maker_ata_b, false),
            AccountMeta::new_readonly(ata_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program, false)
        ]
    );

    let expected_escrow = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: mint_a.to_bytes(),
        mint_b: mint_b.to_bytes(),
        receive: 10 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };
    let serialized_expected_escrow = unsafe { to_bytes::<Escrow>(&expected_escrow) };

    let expected_taker_ata_a = SPLTokenAccount {
        mint: mint_a,
        owner: taker,
        amount: 15 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_taker_ata_a = {
        data_to_bytes::<SPLTokenAccount>(expected_taker_ata_a)
    };

    let expected_taker_ata_b = SPLTokenAccount {
        mint: mint_b,
        owner: taker,
        amount: 5 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_taker_ata_b = {
        data_to_bytes::<SPLTokenAccount>(expected_taker_ata_b)
    };

    let expected_maker_ata_b = SPLTokenAccount {
        mint: mint_b,
        owner: maker,
        amount: 10 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_maker_ata_b = {
        data_to_bytes::<SPLTokenAccount>(expected_maker_ata_b)
    };

    mollusk.process_and_validate_instruction(
        &instruction,
        &vec![
            (taker, taker_account),
            (maker, maker_account),
            (escrow, escrow_account),
            (mint_a, mint_a_account),
            (mint_b, mint_b_account),
            (vault, vault_account),
            (taker_ata_a, taker_ata_a_account),
            (taker_ata_b, taker_ata_b_account),
            (maker_ata_b, maker_ata_b_account),
            (ata_program, ata_program_account),
            (token_program, token_program_account),
            (system_program, system_account)
        ],
        &[
            Check::success(),
            Check::account(&escrow).data(serialized_expected_escrow).build(),
            Check::account(&vault).data(&[]).build(),
            Check::account(&taker_ata_a).data(serialized_expected_taker_ata_a.as_slice()).build(),
            Check::account(&taker_ata_b).data(serialized_expected_taker_ata_b.as_slice()).build(),
            Check::account(&maker_ata_b).data(serialized_expected_maker_ata_b.as_slice()).build(),
        ]
    );
}

#[test]
fn test_refund() {
    let mollusk = mollusk();
    let (system_program, system_account) = mollusk_svm::program::keyed_account_for_system_program();

    let (token_program, token_program_account) = mollusk_svm_programs_token::token::keyed_account();

    let (ata_program, ata_program_account) =
        mollusk_svm_programs_token::associated_token::keyed_account();

    let maker = Pubkey::new_from_array([0x01; 32]);
    let maker_account = Account::new(10 * LAMPORTS_PER_SOL, 0, &system_program);

    let mint_a = Pubkey::new_from_array([0x02; 32]);
    let mut mint_a_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_a_account.data_as_mut_slice()
    ).unwrap();

    let mint_b = Pubkey::new_from_array([0x03; 32]);
    let mut mint_b_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(Mint::LEN),
        Mint::LEN,
        &token_program
    );
    SPLMint::pack(
        SPLMint {
            mint_authority: COption::None,
            supply: 100_000_000,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        },
        mint_b_account.data_as_mut_slice()
    ).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[Escrow::SEED.as_bytes(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );
    let escrow_account = Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: (
            unsafe {
                to_bytes::<Escrow>(
                    &(Escrow {
                        is_initialized: true,
                        seed,
                        maker: maker.to_bytes(),
                        mint_a: mint_a.to_bytes(),
                        mint_b: mint_b.to_bytes(),
                        receive: 10 * LAMPORTS_PER_SOL,
                        bump: escrow_bump,
                    })
                )
            }
        ).to_vec(),
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    };

    let mut vault_account = Account::new(
        mollusk.sysvars.rent.minimum_balance(TokenAccount::LEN),
        TokenAccount::LEN,
        &token_program
    );
    let vault = get_associated_token_address(&escrow, &mint_a);

    SPLTokenAccount::pack(
        SPLTokenAccount {
            mint: mint_a,
            owner: escrow,
            amount: 15 * LAMPORTS_PER_SOL,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        },
        vault_account.data_as_mut_slice()
    ).unwrap();

    let maker_ata_a_account = Account::new(0, 0, &system_program);
    let maker_ata_a = get_associated_token_address(&maker, &mint_a);

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &vec![*Refund::DISCRIMINATOR],
        vec![
            AccountMeta::new(maker, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(mint_a, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(maker_ata_a, false),
            AccountMeta::new_readonly(ata_program, false),
            AccountMeta::new_readonly(token_program, false),
            AccountMeta::new_readonly(system_program, false)
        ]
    );

    let expected_escrow = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: mint_a.to_bytes(),
        mint_b: mint_b.to_bytes(),
        receive: 10 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };
    let serialized_expected_escrow = unsafe { to_bytes::<Escrow>(&expected_escrow) };

    let expected_maker_ata_a = SPLTokenAccount {
        mint: mint_a,
        owner: maker,
        amount: 15 * LAMPORTS_PER_SOL,
        delegate: COption::None,
        state: AccountState::Initialized,
        is_native: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let serialized_expected_maker_ata_a = {
        data_to_bytes::<SPLTokenAccount>(expected_maker_ata_a)
    };

    mollusk.process_and_validate_instruction(
        &instruction,
        &vec![
            (maker, maker_account),
            (escrow, escrow_account),
            (mint_a, mint_a_account),
            (vault, vault_account),
            (maker_ata_a, maker_ata_a_account),
            (ata_program, ata_program_account),
            (token_program, token_program_account),
            (system_program, system_account)
        ],
        &[
            Check::success(),
            Check::account(&escrow).data(serialized_expected_escrow).build(),
            Check::account(&vault).data(&[]).build(),
            Check::account(&maker_ata_a).data(serialized_expected_maker_ata_a.as_slice()).build(),
        ]
    );
}

fn data_to_bytes<T: Pack>(data: T) -> Vec<u8> {
    let mut bytes = vec![0; T::LEN];
    T::pack(data, &mut bytes).unwrap();
    bytes
}
