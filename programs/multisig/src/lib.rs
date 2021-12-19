//! An example of a multisig to execute arbitrary Solana transactions.
//!
//! This program can be used to allow a multisig to govern anything a regular
//! Pubkey can govern. One can use the multisig as a BPF program upgrade
//! authority, a mint authority, etc.
//!
//! To use, one must first create a `Multisig` account, specifying two important
//! parameters:
//!
//! 1. Owners - the set of addresses that sign transactions for the multisig.
//! 2. Threshold - the number of signers required to execute a transaction.
//!
//! Once the `Multisig` account is created, one can create a `Transaction`
//! account, specifying the parameters for a normal solana transaction.
//!
//! To sign, owners should invoke the `approve` instruction, and finally,
//! the `execute_transaction`, once enough (i.e. `threhsold`) of the owners have
//! signed.

use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::instruction::Instruction;
use std::convert::Into;

declare_id!("87CMnS1XEzpePDoXa3HwexwacdUMKubdwbVrPF3djoQJ");

// ***** Program Account ***** //
#[program]
pub mod serum_multisig {
    use super::*;

    // Initializes a new multisig account with a set of owners and a threshold.
    // Owners: List of pubkeys that hold the contract participants
    // Threshold: The threshold of owner votes that msut be reached for consensus
    // Nonce: The PDA address of the Multisig account
    // owner_set_seqno: The initialized value of the number of times the set of owners have changed
    pub fn create_multisig(
        ctx: Context<CreateMultisig>,
        description: String,
        owners: Vec<Pubkey>,
        threshold: u64,
        nonce: u8,
    ) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;
        multisig.description = description;
        multisig.owners = owners;
        multisig.threshold = threshold;
        multisig.nonce = nonce;
        multisig.owner_set_seqno = 0;
        multisig.lamports = 0;
        Ok(())
    }

    // TODO: Document
    // Creates a new transaction account, automatically signed by the creator,
    // which must be one of the owners of the multisig.
    pub fn create_transaction(
        ctx: Context<CreateTransaction>,
        pid: Pubkey,
        accs: Vec<TransactionAccount>,
        data: Vec<u8>,
    ) -> Result<()> {
        let owner_index = ctx
            .accounts
            .multisig
            .owners
            .iter()
            .position(|a| a == ctx.accounts.proposer.key)
            .ok_or(ErrorCode::InvalidOwner)?;

        let mut signers = Vec::new();
        signers.resize(ctx.accounts.multisig.owners.len(), false);
        signers[owner_index] = true;

        let tx = &mut ctx.accounts.transaction;
        tx.program_id = pid;
        tx.accounts = accs;
        tx.data = data;
        tx.signers = signers;
        tx.multisig = *ctx.accounts.multisig.to_account_info().key;
        tx.did_execute = false;
        tx.owner_set_seqno = ctx.accounts.multisig.owner_set_seqno;

        Ok(())
    }

    // TODO: Document
    // Approves a transaction on behalf of an owner of the multisig.
    pub fn approve(ctx: Context<Approve>) -> Result<()> {
        let owner_index = ctx
            .accounts
            .multisig
            .owners
            .iter()
            .position(|a| a == ctx.accounts.owner.key)
            .ok_or(ErrorCode::InvalidOwner)?;

        ctx.accounts.transaction.signers[owner_index] = true;

        Ok(())
    }

    // TODO: Document
    // Set owners and threshold at once.
    pub fn set_owners_and_change_threshold<'info>(
        ctx: Context<'_, '_, '_, 'info, Auth<'info>>,
        owners: Vec<Pubkey>,
        threshold: u64,
    ) -> Result<()> {
        set_owners(
            Context::new(ctx.program_id, ctx.accounts, ctx.remaining_accounts),
            owners,
        )?;
        change_threshold(ctx, threshold)
    }

    // Sets the owners field on the multisig. The only way this can be invoked
    // is via a recursive call from execute_transaction -> set_owners.
    pub fn set_owners(ctx: Context<Auth>, owners: Vec<Pubkey>) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;

        if (owners.len() as u64) < multisig.threshold {
            multisig.threshold = owners.len() as u64;
        }

        multisig.owners = owners;
        multisig.owner_set_seqno += 1;

        Ok(())
    }

    // Deposit lamports into the multisig account.
    // Can only be done recursively through execute_transaction -> deposit_lamports
    pub fn deposit_lamports(ctx:Context<Escrow>, lamports: u64 )-> Result<()>{
        Ok(())
    }

    // Withdraw lamports to the owner parties.
    // Can only be done recursively through execute_transaction -> withdraw_lamports
    pub fn withdraw_lamports(ctx:Context<Escrow>)-> Result<()>{
        Ok(())
    }

    // Changes the execution threshold of the multisig. The only way this can be
    // invoked is via a recursive call from execute_transaction ->
    // change_threshold.
    pub fn change_threshold(ctx: Context<Auth>, threshold: u64) -> Result<()> {
        if threshold > ctx.accounts.multisig.owners.len() as u64 {
            return Err(ErrorCode::InvalidThreshold.into());
        }
        let multisig = &mut ctx.accounts.multisig;
        multisig.threshold = threshold;
        Ok(())
    }

    // TODO: Document
    // Executes the given transaction if threshold owners have signed it.
    pub fn execute_transaction(ctx: Context<ExecuteTransaction>) -> Result<()> {
        // Has this been executed already?
        if ctx.accounts.transaction.did_execute {
            return Err(ErrorCode::AlreadyExecuted.into());
        }

        // Get the count of valid signers on the pending transaction
        let sig_count = ctx
            .accounts
            .transaction
            .signers
            .iter()
            .filter(|&did_sign| *did_sign)
            .count() as u64;

        // Do we have enough signers on the transaction to execute?    
        if sig_count < ctx.accounts.multisig.threshold {
            return Err(ErrorCode::NotEnoughSigners.into());
        }

        // Turn the transaction account into a Instruction type
        let mut ix: Instruction = (&*ctx.accounts.transaction).into();

        // Grab the metadata for what accounts should be passed to the instruction processor
        // In this case we only want the multisig_signer Program Derived Address that we created with the programId and the multisig publicKey
        ix.accounts = ix
            .accounts
            .iter()
            .map(|acc| {
                let mut acc = acc.clone();
                if &acc.pubkey == ctx.accounts.multisig_signer.key {
                    acc.is_signer = true;
                }
                acc
            })
            .collect();

        // Generate the seeds to find the multisig_signer Program Derived Address
        let seeds = &[
            ctx.accounts.multisig.to_account_info().key.as_ref(),
            &[ctx.accounts.multisig.nonce],
        ];

        // The one signer needed to execute
        let signer = &[&seeds[..]];

        // Grab all of the accounts that havent been touched
        let accounts = ctx.remaining_accounts;

        // Invoke a cross-program instruction with program signatures
        solana_program::program::invoke_signed(&ix, accounts, signer)?;

        // Burn the transaction to ensure one time use.
        ctx.accounts.transaction.did_execute = true;

        Ok(())
    }
}

// ***** Contexts ***** //
#[derive(Accounts)]
pub struct CreateMultisig<'info> {
    #[account(zero)]
    multisig: ProgramAccount<'info, Multisig>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct CreateTransaction<'info> {
    multisig: ProgramAccount<'info, Multisig>,
    #[account(zero)]
    transaction: ProgramAccount<'info, Transaction>,
    // One of the owners. Checked in the handler.
    #[account(signer)]
    proposer: AccountInfo<'info>,
    rent: Sysvar<'info, Rent>,
}

// TODO: Document
#[derive(Accounts)]
pub struct Approve<'info> {
    #[account(constraint = multisig.owner_set_seqno == transaction.owner_set_seqno)]
    multisig: ProgramAccount<'info, Multisig>,
    #[account(mut, has_one = multisig)]
    transaction: ProgramAccount<'info, Transaction>,
    // One of the multisig owners. Checked in the handler.
    #[account(signer)]
    owner: AccountInfo<'info>,
}

// TODO: Document
#[derive(Accounts)]
pub struct Escrow<'info> {
    #[account(constraint = multisig.owner_set_seqno == transaction.owner_set_seqno)]
    multisig: ProgramAccount<'info, Multisig>,
    #[account(mut, has_one = multisig)]
    transaction: ProgramAccount<'info, Transaction>,
    // One of the multisig owners. Checked in the handler.
    #[account(signer)]
    owner: AccountInfo<'info>,
}

// TODO: Document
#[derive(Accounts)]
pub struct Auth<'info> {
    #[account(mut)]
    multisig: ProgramAccount<'info, Multisig>,
    #[account(
        signer,
        seeds = [multisig.to_account_info().key.as_ref()],
        bump = multisig.nonce,
    )]
    multisig_signer: AccountInfo<'info>,
}

// TODO: Document
#[derive(Accounts)]
pub struct ExecuteTransaction<'info> {
    #[account(constraint = multisig.owner_set_seqno == transaction.owner_set_seqno)]
    multisig: ProgramAccount<'info, Multisig>,
    #[account(
        seeds = [multisig.to_account_info().key.as_ref()],
        bump = multisig.nonce,
    )]
    multisig_signer: AccountInfo<'info>,
    #[account(mut, has_one = multisig)]
    transaction: ProgramAccount<'info, Transaction>,
}

// ***** Data Accounts ***** //
// TODO: Document
#[account]
pub struct Multisig {
    pub description: String,
    pub owners: Vec<Pubkey>,
    pub threshold: u64,
    pub nonce: u8,
    pub owner_set_seqno: u32,
    pub lamports: u64,
}

// TODO: Document
#[account]
pub struct Transaction {
    // The multisig account this transaction belongs to.
    pub multisig: Pubkey,
    // Target program to execute against.
    pub program_id: Pubkey,
    // Accounts requried for the transaction.
    pub accounts: Vec<TransactionAccount>,
    // Instruction data for the transaction.
    pub data: Vec<u8>,
    // signers[index] is true iff multisig.owners[index] signed the transaction.
    pub signers: Vec<bool>,
    // Boolean ensuring one time execution.
    pub did_execute: bool,
    // Owner set sequence number.
    pub owner_set_seqno: u32,
}

// We implement the From trait for the Instruction type in order to turn a Transaction type into an Instruction type
// We consume the Transaction type and convert it into an Instruction type
impl From<&Transaction> for Instruction {
    fn from(tx: &Transaction) -> Instruction {
        Instruction {
            program_id: tx.program_id,
            accounts: tx.accounts.iter().map(AccountMeta::from).collect(),
            data: tx.data.clone(),
        }
    }
}

// Accounts that are a aprt of the wrapped transactions that will execute once we have enough votes
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TransactionAccount {
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
}

// We implement the From trait for the TransactionAccount type in order to convert it into an AccountMeta type
impl From<&TransactionAccount> for AccountMeta {
    fn from(account: &TransactionAccount) -> AccountMeta {
        match account.is_writable {
            false => AccountMeta::new_readonly(account.pubkey, account.is_signer),
            true => AccountMeta::new(account.pubkey, account.is_signer),
        }
    }
}

// We implement the From trait for the AccountMeta type in order to convert it into an TransactionAccount type
impl From<&AccountMeta> for TransactionAccount {
    fn from(account_meta: &AccountMeta) -> TransactionAccount {
        TransactionAccount {
            pubkey: account_meta.pubkey,
            is_signer: account_meta.is_signer,
            is_writable: account_meta.is_writable,
        }
    }
}

// ***** Errors ***** //
#[error]
pub enum ErrorCode {
    #[msg("The given owner is not part of this multisig.")]
    InvalidOwner,
    #[msg("Not enough owners signed this transaction.")]
    NotEnoughSigners,
    #[msg("Cannot delete a transaction that has been signed by an owner.")]
    TransactionAlreadySigned,
    #[msg("Overflow when adding.")]
    Overflow,
    #[msg("Cannot delete a transaction the owner did not create.")]
    UnableToDelete,
    #[msg("The given transaction has already been executed.")]
    AlreadyExecuted,
    #[msg("Threshold must be less than or equal to the number of owners.")]
    InvalidThreshold,
    // TODO: add new errors for depositing and withdrawing lamports
}
