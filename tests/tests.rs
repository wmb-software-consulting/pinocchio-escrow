use solana_sdk::account::Account;
use solana_sdk::message::Message;
use solana_sdk::native_token::LAMPORTS_PER_SOL;
use solana_sdk::program_option::COption;
use solana_sdk::program_pack::Pack;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::transaction::Transaction;
use solana_sdk::msg;
use solana_sdk::rent::Rent;
use solana_sdk::{
    instruction::{ AccountMeta, Instruction },
    pubkey::Pubkey,
    system_instruction,
    system_program,
};

use pinocchio_token::{ ID as TOKEN_PROGRAM_ID };
use pinocchio_associated_token_account::{ ID as ATA_PROGRAM_ID };

use pinocchio_escrow::{ load_acc, to_bytes, Escrow, Make, MakeInstructionData, Refund, Take, ID };

use litesvm::LiteSVM;
use spl_associated_token_account_client::address::get_associated_token_address;
use spl_associated_token_account_client::instruction::create_associated_token_account;
use spl_token::instruction::TokenInstruction;
use spl_token::{ state::{ Account as SPLTokenAccount, Mint as SPLMint } };
use solana_hash::Hash;

pub const PROGRAM: Pubkey = Pubkey::new_from_array(ID);

#[test]
fn test_make() {
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!("../target/deploy/pinocchio_escrow.so");
    svm.add_program(PROGRAM, bytes);

    let maker_keypair = Keypair::new();
    let maker = maker_keypair.pubkey();

    let mint_authority = Keypair::new();
    svm.airdrop(&maker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"escrow".as_slice(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );

    let (tx, usdc_mint, maker_ata) = mint(
        &mint_authority,
        &maker_keypair,
        30 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let vault = get_associated_token_address(&escrow, &usdc_mint);

    let mint_b_pubkey = Pubkey::new_from_array([0x3; 32]);
    let mint_b = SPLMint {
        mint_authority: COption::None,
        supply: 1_000_000_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let mut mint_b_bytes = [0u8; SPLMint::LEN];
    SPLMint::pack(mint_b, &mut mint_b_bytes).unwrap();
    svm.set_account(mint_b_pubkey, Account {
        lamports: 1_000_000_000,
        data: mint_b_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let instruction_data = MakeInstructionData {
        seed,
        amount: 10 * LAMPORTS_PER_SOL,
        receive: 10,
    };

    let mut ser_instruction_data = vec![*Make::DISCRIMINATOR];
    ser_instruction_data.extend_from_slice(unsafe {
        to_bytes::<MakeInstructionData>(&instruction_data)
    });

    msg!("Serialized instruction data: {:?}", ser_instruction_data);

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &ser_instruction_data,
        vec![
            AccountMeta::new(maker, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new(vault, false),
            AccountMeta::new(maker_ata, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new_readonly(mint_b_pubkey, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(ATA_PROGRAM_ID), false),
            AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
            AccountMeta::new_readonly(system_program::ID, false)
        ]
    );

    let tx = Transaction::new(
        &[&maker_keypair],
        Message::new(&[instruction], Some(&maker)),
        svm.latest_blockhash()
    );

    let maker_ata_info = svm.get_account(&maker_ata).unwrap();
    let maker_ata_data = SPLTokenAccount::unpack(&maker_ata_info.data).unwrap();
    assert_eq!(maker_ata_data.owner, maker);
    assert_eq!(maker_ata_data.mint, usdc_mint);
    assert_eq!(maker_ata_data.amount, 30 * LAMPORTS_PER_SOL);

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let escrow_account = svm.get_account(&escrow);
    let escrow_account_data = unsafe {
        *load_acc::<Escrow>(&mut escrow_account.unwrap().data).unwrap()
    };
    assert_eq!(escrow_account_data.is_initialized, true);
    assert_eq!(escrow_account_data.seed, seed);
    assert_eq!(escrow_account_data.maker, maker.to_bytes());
    assert_eq!(escrow_account_data.mint_a, usdc_mint.to_bytes());
    assert_eq!(escrow_account_data.mint_b, mint_b_pubkey.to_bytes());
    assert_eq!(escrow_account_data.receive, 10);
    assert_eq!(escrow_account_data.bump, escrow_bump);

    let token_account_info = svm.get_account(&maker_ata).unwrap();
    let token_data = SPLTokenAccount::unpack(&token_account_info.data).unwrap();
    assert_eq!(token_data.owner, maker);
    assert_eq!(token_data.mint, usdc_mint);
    assert_eq!(token_data.amount, 20 * LAMPORTS_PER_SOL);

    let vault_ata_info = svm.get_account(&vault).unwrap();
    let vault_ata_data = SPLTokenAccount::unpack(&vault_ata_info.data).unwrap();
    assert_eq!(vault_ata_data.owner, escrow);
    assert_eq!(vault_ata_data.mint, usdc_mint);
    assert_eq!(vault_ata_data.amount, 10 * LAMPORTS_PER_SOL);
}

#[test]
fn test_take() {
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!("../target/deploy/pinocchio_escrow.so");
    svm.add_program(PROGRAM, bytes);

    let maker_keypair = Keypair::new();
    let maker = maker_keypair.pubkey();

    let taker_keypair = Keypair::new();
    let taker = taker_keypair.pubkey();

    let mint_authority = Keypair::new();
    svm.airdrop(&maker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&taker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"escrow".as_slice(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );

    let (tx, usdc_mint, maker_ata_a_pubkey) = mint(
        &mint_authority,
        &maker_keypair,
        30 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let mint_b_pubkey = Pubkey::new_from_array([0x3; 32]);
    let mint_b = SPLMint {
        mint_authority: COption::None,
        supply: 1_000_000_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let mut mint_b_bytes = [0u8; SPLMint::LEN];
    SPLMint::pack(mint_b, &mut mint_b_bytes).unwrap();
    svm.set_account(mint_b_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: mint_b_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let escrow_data = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: usdc_mint.to_bytes(),
        mint_b: mint_b_pubkey.to_bytes(),
        receive: 15 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };

    svm.set_account(escrow, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: unsafe {
            to_bytes(&escrow_data).to_vec()
        },
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let vault_data = SPLTokenAccount {
        is_native: COption::None,
        mint: usdc_mint,
        owner: escrow,
        amount: 10 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut vault_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(vault_data, &mut vault_data_bytes).unwrap();

    let vault_pubkey = get_associated_token_address(&escrow, &usdc_mint);
    svm.set_account(vault_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: vault_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let taker_ata_a_pubkey = get_associated_token_address(&taker, &usdc_mint);

    let taker_ata_b_pubkey = get_associated_token_address(&taker, &mint_b_pubkey);
    let taker_ata_b_data = SPLTokenAccount {
        is_native: COption::None,
        mint: mint_b_pubkey,
        owner: taker,
        amount: 15 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut taker_ata_b_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(taker_ata_b_data, &mut taker_ata_b_data_bytes).unwrap();

    svm.set_account(taker_ata_b_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: taker_ata_b_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let maker_ata_b_pubkey = get_associated_token_address(&maker, &mint_b_pubkey);

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &[*Take::DISCRIMINATOR],
        vec![
            AccountMeta::new(taker, true),
            AccountMeta::new(maker, false),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new_readonly(mint_b_pubkey, false),
            AccountMeta::new(vault_pubkey, false),
            AccountMeta::new(taker_ata_a_pubkey, false),
            AccountMeta::new(taker_ata_b_pubkey, false),
            AccountMeta::new(maker_ata_b_pubkey, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(ATA_PROGRAM_ID), false),
            AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
            AccountMeta::new_readonly(system_program::ID, false)
        ]
    );

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let escrow_account = svm.get_account(&escrow);
    let escrow_account_data = unsafe {
        *load_acc::<Escrow>(&mut escrow_account.unwrap().data).unwrap()
    };
    assert_eq!(escrow_account_data.is_initialized, true);
    assert_eq!(escrow_account_data.seed, seed);
    assert_eq!(escrow_account_data.maker, maker.to_bytes());
    assert_eq!(escrow_account_data.mint_a, usdc_mint.to_bytes());
    assert_eq!(escrow_account_data.mint_b, mint_b_pubkey.to_bytes());
    assert_eq!(escrow_account_data.receive, 15 * LAMPORTS_PER_SOL);
    assert_eq!(escrow_account_data.bump, escrow_bump);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    let vault_ata_data = SPLTokenAccount::unpack(&vault_ata_info.data).unwrap();
    assert_eq!(vault_ata_data.owner, escrow);
    assert_eq!(vault_ata_data.mint, usdc_mint);
    assert_eq!(vault_ata_data.amount, 10 * LAMPORTS_PER_SOL);

    let taker_ata_b_info = svm.get_account(&taker_ata_b_pubkey).unwrap();
    let taker_ata_b_data = SPLTokenAccount::unpack(&taker_ata_b_info.data).unwrap();
    assert_eq!(taker_ata_b_data.owner, taker);
    assert_eq!(taker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(taker_ata_b_data.amount, 15 * LAMPORTS_PER_SOL);

    let tx = Transaction::new(
        &[&taker_keypair],
        Message::new(&[instruction], Some(&taker)),
        svm.latest_blockhash()
    );
    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let maker_ata_b_info = svm.get_account(&maker_ata_b_pubkey).unwrap();
    let maker_ata_b_data = SPLTokenAccount::unpack(&maker_ata_b_info.data).unwrap();
    assert_eq!(maker_ata_b_data.owner, maker);
    assert_eq!(maker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(maker_ata_b_data.amount, 15 * LAMPORTS_PER_SOL);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    assert_eq!(vault_ata_info.lamports, 0);
    assert!(vault_ata_info.data.is_empty());

    let taker_ata_a_info = svm.get_account(&taker_ata_a_pubkey).unwrap();
    let taker_ata_a_data = SPLTokenAccount::unpack(&taker_ata_a_info.data).unwrap();
    assert_eq!(taker_ata_a_data.owner, taker);
    assert_eq!(taker_ata_a_data.mint, usdc_mint);
    assert_eq!(taker_ata_a_data.amount, 10 * LAMPORTS_PER_SOL);

    let taker_ata_b_info = svm.get_account(&taker_ata_b_pubkey).unwrap();
    let taker_ata_b_data = SPLTokenAccount::unpack(&taker_ata_b_info.data).unwrap();
    assert_eq!(taker_ata_b_data.owner, taker);
    assert_eq!(taker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(taker_ata_b_data.amount, 0 * LAMPORTS_PER_SOL);
}

#[test]
fn test_take_with_initiated_ata() {
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!("../target/deploy/pinocchio_escrow.so");
    svm.add_program(PROGRAM, bytes);

    let maker_keypair = Keypair::new();
    let maker = maker_keypair.pubkey();

    let taker_keypair = Keypair::new();
    let taker = taker_keypair.pubkey();

    let mint_authority = Keypair::new();
    svm.airdrop(&maker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&taker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"escrow".as_slice(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );

    let (tx, usdc_mint, maker_ata_a_pubkey) = mint(
        &mint_authority,
        &maker_keypair,
        30 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let mint_b_pubkey = Pubkey::new_from_array([0x3; 32]);
    let mint_b = SPLMint {
        mint_authority: COption::None,
        supply: 1_000_000_000_000,
        decimals: 6,
        is_initialized: true,
        freeze_authority: COption::None,
    };

    let mut mint_b_bytes = [0u8; SPLMint::LEN];
    SPLMint::pack(mint_b, &mut mint_b_bytes).unwrap();
    svm.set_account(mint_b_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: mint_b_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let escrow_data = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: usdc_mint.to_bytes(),
        mint_b: mint_b_pubkey.to_bytes(),
        receive: 15 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };

    svm.set_account(escrow, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: unsafe {
            to_bytes(&escrow_data).to_vec()
        },
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let vault_data = SPLTokenAccount {
        is_native: COption::None,
        mint: usdc_mint,
        owner: escrow,
        amount: 10 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut vault_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(vault_data, &mut vault_data_bytes).unwrap();

    let vault_pubkey = get_associated_token_address(&escrow, &usdc_mint);
    svm.set_account(vault_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: vault_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let taker_ata_a_pubkey = get_associated_token_address(&taker, &usdc_mint);
    let taker_ata_a_data = SPLTokenAccount {
        is_native: COption::None,
        mint: usdc_mint,
        owner: taker,
        amount: 15 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut taker_ata_a_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(taker_ata_a_data, &mut taker_ata_a_data_bytes).unwrap();

    svm.set_account(taker_ata_a_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: taker_ata_a_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let taker_ata_b_pubkey = get_associated_token_address(&taker, &mint_b_pubkey);
    let taker_ata_b_data = SPLTokenAccount {
        is_native: COption::None,
        mint: mint_b_pubkey,
        owner: taker,
        amount: 15 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut taker_ata_b_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(taker_ata_b_data, &mut taker_ata_b_data_bytes).unwrap();

    svm.set_account(taker_ata_b_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: taker_ata_b_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let maker_ata_b_pubkey = get_associated_token_address(&maker, &mint_b_pubkey);
    let maker_ata_b_data = SPLTokenAccount {
        is_native: COption::None,
        mint: mint_b_pubkey,
        owner: maker,
        amount: 1 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut maker_ata_b_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(maker_ata_b_data, &mut maker_ata_b_data_bytes).unwrap();

    svm.set_account(maker_ata_b_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: maker_ata_b_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &[*Take::DISCRIMINATOR],
        vec![
            AccountMeta::new(taker, true),
            AccountMeta::new(maker, false),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new_readonly(mint_b_pubkey, false),
            AccountMeta::new(vault_pubkey, false),
            AccountMeta::new(taker_ata_a_pubkey, false),
            AccountMeta::new(taker_ata_b_pubkey, false),
            AccountMeta::new(maker_ata_b_pubkey, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(ATA_PROGRAM_ID), false),
            AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
            AccountMeta::new_readonly(system_program::ID, false)
        ]
    );

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let maker_ata_b_info = svm.get_account(&maker_ata_b_pubkey).unwrap();
    let maker_ata_b_data = SPLTokenAccount::unpack(&maker_ata_b_info.data).unwrap();
    assert_eq!(maker_ata_b_data.owner, maker);
    assert_eq!(maker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(maker_ata_b_data.amount, 1 * LAMPORTS_PER_SOL);

    let escrow_account = svm.get_account(&escrow);
    let escrow_account_data = unsafe {
        *load_acc::<Escrow>(&mut escrow_account.unwrap().data).unwrap()
    };
    assert_eq!(escrow_account_data.is_initialized, true);
    assert_eq!(escrow_account_data.seed, seed);
    assert_eq!(escrow_account_data.maker, maker.to_bytes());
    assert_eq!(escrow_account_data.mint_a, usdc_mint.to_bytes());
    assert_eq!(escrow_account_data.mint_b, mint_b_pubkey.to_bytes());
    assert_eq!(escrow_account_data.receive, 15 * LAMPORTS_PER_SOL);
    assert_eq!(escrow_account_data.bump, escrow_bump);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    let vault_ata_data = SPLTokenAccount::unpack(&vault_ata_info.data).unwrap();
    assert_eq!(vault_ata_data.owner, escrow);
    assert_eq!(vault_ata_data.mint, usdc_mint);
    assert_eq!(vault_ata_data.amount, 10 * LAMPORTS_PER_SOL);

    let taker_ata_a_info = svm.get_account(&taker_ata_a_pubkey).unwrap();
    let taker_ata_a_data = SPLTokenAccount::unpack(&taker_ata_a_info.data).unwrap();
    assert_eq!(taker_ata_a_data.owner, taker);
    assert_eq!(taker_ata_a_data.mint, usdc_mint);
    assert_eq!(taker_ata_a_data.amount, 15 * LAMPORTS_PER_SOL);

    let taker_ata_b_info = svm.get_account(&taker_ata_b_pubkey).unwrap();
    let taker_ata_b_data = SPLTokenAccount::unpack(&taker_ata_b_info.data).unwrap();
    assert_eq!(taker_ata_b_data.owner, taker);
    assert_eq!(taker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(taker_ata_b_data.amount, 15 * LAMPORTS_PER_SOL);

    let tx = Transaction::new(
        &[&taker_keypair],
        Message::new(&[instruction], Some(&taker)),
        svm.latest_blockhash()
    );
    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let maker_ata_b_info = svm.get_account(&maker_ata_b_pubkey).unwrap();
    let maker_ata_b_data = SPLTokenAccount::unpack(&maker_ata_b_info.data).unwrap();
    assert_eq!(maker_ata_b_data.owner, maker);
    assert_eq!(maker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(maker_ata_b_data.amount, 16 * LAMPORTS_PER_SOL);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    assert_eq!(vault_ata_info.lamports, 0);
    assert!(vault_ata_info.data.is_empty());

    let taker_ata_a_info = svm.get_account(&taker_ata_a_pubkey).unwrap();
    let taker_ata_a_data = SPLTokenAccount::unpack(&taker_ata_a_info.data).unwrap();
    assert_eq!(taker_ata_a_data.owner, taker);
    assert_eq!(taker_ata_a_data.mint, usdc_mint);
    assert_eq!(taker_ata_a_data.amount, 25 * LAMPORTS_PER_SOL);

    let taker_ata_b_info = svm.get_account(&taker_ata_b_pubkey).unwrap();
    let taker_ata_b_data = SPLTokenAccount::unpack(&taker_ata_b_info.data).unwrap();
    assert_eq!(taker_ata_b_data.owner, taker);
    assert_eq!(taker_ata_b_data.mint, mint_b_pubkey);
    assert_eq!(taker_ata_b_data.amount, 0 * LAMPORTS_PER_SOL);
}

#[test]
fn test_refund() {
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!("../target/deploy/pinocchio_escrow.so");
    svm.add_program(PROGRAM, bytes);

    let maker_keypair = Keypair::new();
    let maker = maker_keypair.pubkey();

    let mint_authority = Keypair::new();
    svm.airdrop(&maker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"escrow".as_slice(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );

    let (tx, usdc_mint, _) = mint(
        &mint_authority,
        &maker_keypair,
        30 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let maker_ata_a_pubkey = get_associated_token_address(&maker, &usdc_mint);

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let mint_b_pubkey = Pubkey::new_from_array([0x3; 32]);

    let escrow_data = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: usdc_mint.to_bytes(),
        mint_b: mint_b_pubkey.to_bytes(),
        receive: 15 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };

    svm.set_account(escrow, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: unsafe {
            to_bytes(&escrow_data).to_vec()
        },
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let vault_data = SPLTokenAccount {
        is_native: COption::None,
        mint: usdc_mint,
        owner: escrow,
        amount: 10 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut vault_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(vault_data, &mut vault_data_bytes).unwrap();

    let vault_pubkey = get_associated_token_address(&escrow, &usdc_mint);
    svm.set_account(vault_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: vault_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &[*Refund::DISCRIMINATOR],
        vec![
            AccountMeta::new(maker, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(vault_pubkey, false),
            AccountMeta::new(maker_ata_a_pubkey, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(ATA_PROGRAM_ID), false),
            AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
            AccountMeta::new_readonly(system_program::ID, false)
        ]
    );

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let escrow_account = svm.get_account(&escrow);
    let escrow_account_data = unsafe {
        *load_acc::<Escrow>(&mut escrow_account.unwrap().data).unwrap()
    };
    assert_eq!(escrow_account_data.is_initialized, true);
    assert_eq!(escrow_account_data.seed, seed);
    assert_eq!(escrow_account_data.maker, maker.to_bytes());
    assert_eq!(escrow_account_data.mint_a, usdc_mint.to_bytes());
    assert_eq!(escrow_account_data.mint_b, mint_b_pubkey.to_bytes());
    assert_eq!(escrow_account_data.receive, 15 * LAMPORTS_PER_SOL);
    assert_eq!(escrow_account_data.bump, escrow_bump);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    let vault_ata_data = SPLTokenAccount::unpack(&vault_ata_info.data).unwrap();
    assert_eq!(vault_ata_data.owner, escrow);
    assert_eq!(vault_ata_data.mint, usdc_mint);
    assert_eq!(vault_ata_data.amount, 10 * LAMPORTS_PER_SOL);

    let tx = Transaction::new(
        &[&maker_keypair],
        Message::new(&[instruction], Some(&maker)),
        svm.latest_blockhash()
    );
    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 40 * LAMPORTS_PER_SOL);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    assert_eq!(vault_ata_info.lamports, 0);
    assert!(vault_ata_info.data.is_empty());
}

#[test]
fn test_refund_with_initiated_ata() {
    let mut svm = LiteSVM::new();

    let bytes = include_bytes!("../target/deploy/pinocchio_escrow.so");
    svm.add_program(PROGRAM, bytes);

    let maker_keypair = Keypair::new();
    let maker = maker_keypair.pubkey();

    let mint_authority = Keypair::new();
    svm.airdrop(&maker, 100 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();

    let seed = 123456789u64;
    let (escrow, escrow_bump) = solana_sdk::pubkey::Pubkey::find_program_address(
        &[b"escrow".as_slice(), &maker.to_bytes(), seed.to_le_bytes().as_ref()],
        &PROGRAM
    );

    let (tx, usdc_mint, maker_ata_a_pubkey) = mint(
        &mint_authority,
        &maker_keypair,
        30 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let mint_b_pubkey = Pubkey::new_from_array([0x3; 32]);

    let escrow_data = Escrow {
        is_initialized: true,
        seed,
        maker: maker.to_bytes(),
        mint_a: usdc_mint.to_bytes(),
        mint_b: mint_b_pubkey.to_bytes(),
        receive: 15 * LAMPORTS_PER_SOL,
        bump: escrow_bump,
    };

    svm.set_account(escrow, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: unsafe {
            to_bytes(&escrow_data).to_vec()
        },
        owner: PROGRAM,
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let vault_data = SPLTokenAccount {
        is_native: COption::None,
        mint: usdc_mint,
        owner: escrow,
        amount: 10 * LAMPORTS_PER_SOL,
        state: spl_token::state::AccountState::Initialized,
        delegate: COption::None,
        delegated_amount: 0,
        close_authority: COption::None,
    };

    let mut vault_data_bytes = [0u8; SPLTokenAccount::LEN];
    SPLTokenAccount::pack(vault_data, &mut vault_data_bytes).unwrap();

    let vault_pubkey = get_associated_token_address(&escrow, &usdc_mint);
    svm.set_account(vault_pubkey, Account {
        lamports: 10 * LAMPORTS_PER_SOL,
        data: vault_data_bytes.to_vec(),
        owner: Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        executable: false,
        rent_epoch: 0,
    }).unwrap();

    let instruction = Instruction::new_with_bytes(
        PROGRAM,
        &[*Refund::DISCRIMINATOR],
        vec![
            AccountMeta::new(maker, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(usdc_mint, false),
            AccountMeta::new(vault_pubkey, false),
            AccountMeta::new(maker_ata_a_pubkey, false),
            AccountMeta::new_readonly(Pubkey::new_from_array(ATA_PROGRAM_ID), false),
            AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
            AccountMeta::new_readonly(system_program::ID, false)
        ]
    );

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 30 * LAMPORTS_PER_SOL);

    let escrow_account = svm.get_account(&escrow);
    let escrow_account_data = unsafe {
        *load_acc::<Escrow>(&mut escrow_account.unwrap().data).unwrap()
    };
    assert_eq!(escrow_account_data.is_initialized, true);
    assert_eq!(escrow_account_data.seed, seed);
    assert_eq!(escrow_account_data.maker, maker.to_bytes());
    assert_eq!(escrow_account_data.mint_a, usdc_mint.to_bytes());
    assert_eq!(escrow_account_data.mint_b, mint_b_pubkey.to_bytes());
    assert_eq!(escrow_account_data.receive, 15 * LAMPORTS_PER_SOL);
    assert_eq!(escrow_account_data.bump, escrow_bump);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    let vault_ata_data = SPLTokenAccount::unpack(&vault_ata_info.data).unwrap();
    assert_eq!(vault_ata_data.owner, escrow);
    assert_eq!(vault_ata_data.mint, usdc_mint);
    assert_eq!(vault_ata_data.amount, 10 * LAMPORTS_PER_SOL);

    let tx = Transaction::new(
        &[&maker_keypair],
        Message::new(&[instruction], Some(&maker)),
        svm.latest_blockhash()
    );
    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let maker_ata_a_info = svm.get_account(&maker_ata_a_pubkey).unwrap();
    let maker_ata_a_data = SPLTokenAccount::unpack(&maker_ata_a_info.data).unwrap();
    assert_eq!(maker_ata_a_data.owner, maker);
    assert_eq!(maker_ata_a_data.mint, usdc_mint);
    assert_eq!(maker_ata_a_data.amount, 40 * LAMPORTS_PER_SOL);

    let vault_ata_info = svm.get_account(&vault_pubkey).unwrap();
    assert_eq!(vault_ata_info.lamports, 0);
    assert!(vault_ata_info.data.is_empty());
}

#[test]
fn test_mint_token() {
    let mut svm = LiteSVM::new();

    let mint_authority = Keypair::new();
    let user = Keypair::new();

    svm.airdrop(&mint_authority.pubkey(), 10 * LAMPORTS_PER_SOL).unwrap();
    svm.airdrop(&user.pubkey(), LAMPORTS_PER_SOL).unwrap();

    let (tx, mint_pubkey, ata) = mint(
        &mint_authority,
        &user,
        100 * LAMPORTS_PER_SOL,
        svm.latest_blockhash()
    );

    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Transaction failed: {:?}", result);

    let token_account_info = svm.get_account(&ata).unwrap();
    let token_data = SPLTokenAccount::unpack(&token_account_info.data).unwrap();
    assert_eq!(token_data.owner, user.pubkey());
    assert_eq!(token_data.mint, mint_pubkey);
    assert_eq!(token_data.amount, 100 * LAMPORTS_PER_SOL);
}

fn mint(
    mint_authority: &Keypair,
    user: &Keypair,
    amount: u64,
    latest_blockhash: Hash
) -> (Transaction, Pubkey, Pubkey) {
    let mint_keypair = Keypair::new();
    let mint_pubkey = mint_keypair.pubkey();

    let mint_rent = Rent::default().minimum_balance(SPLMint::LEN);

    let create_account_ix = system_instruction::create_account(
        &mint_authority.pubkey(),
        &mint_pubkey,
        mint_rent,
        SPLMint::LEN as u64,
        &Pubkey::new_from_array(TOKEN_PROGRAM_ID)
    );

    let data = (TokenInstruction::InitializeMint {
        decimals: 6,
        mint_authority: mint_authority.pubkey(),
        freeze_authority: COption::None,
    }).pack();

    let initialize_mint_ix = Instruction::new_with_bytes(
        Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        &data,
        vec![
            AccountMeta::new(mint_pubkey, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false)
        ]
    );

    let ata = get_associated_token_address(&user.pubkey(), &mint_pubkey);

    let create_ata_ix = create_associated_token_account(
        &user.pubkey(),
        &user.pubkey(),
        &mint_pubkey,
        &Pubkey::new_from_array(TOKEN_PROGRAM_ID)
    );
    // let create_ata_ix = Instruction::new_with_bytes(
    //     Pubkey::new_from_array(ATA_PROGRAM_ID),
    //     &[],
    //     vec![
    //         AccountMeta::new(user.pubkey(), true),
    //         AccountMeta::new(token_account, false),
    //         AccountMeta::new_readonly(user.pubkey(), false),
    //         AccountMeta::new_readonly(mint_pubkey, false),
    //         AccountMeta::new_readonly(system_program::ID, false),
    //         AccountMeta::new_readonly(Pubkey::new_from_array(TOKEN_PROGRAM_ID), false),
    //         AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false)
    //     ]
    // );
    let ser_mint_to_ix = (TokenInstruction::MintTo {
        amount: amount,
    }).pack();
    let mint_to_ix = Instruction::new_with_bytes(
        Pubkey::new_from_array(TOKEN_PROGRAM_ID),
        &ser_mint_to_ix,
        vec![
            AccountMeta::new(mint_pubkey, false),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(mint_authority.pubkey(), true)
        ]
    );

    let tx = Transaction::new(
        &[&mint_authority, &mint_keypair, &user],
        Message::new(
            &[create_account_ix, initialize_mint_ix, create_ata_ix, mint_to_ix],
            Some(&mint_authority.pubkey())
        ),
        latest_blockhash
    );
    (tx, mint_pubkey, ata)
}
