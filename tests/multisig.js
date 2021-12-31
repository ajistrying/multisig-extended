const anchor = require("@project-serum/anchor");
const assert = require("assert");

describe("multisig", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());

  const program = anchor.workspace.SerumMultisig;

  it("Tests the multisig program", async () => {

    // Generate a new random keypair
    const multisig = anchor.web3.Keypair.generate();

    // Find a valid program address and nonce
    const [
      multisigSigner,
      nonce,
    ] = await anchor.web3.PublicKey.findProgramAddress(
      [multisig.publicKey.toBuffer()],
      program.programId
    );
    const multisigSize = 200; // Big enough.

    // set the owners of the wallet
    const ownerA = anchor.web3.Keypair.generate();
    const ownerB = anchor.web3.Keypair.generate();
    const ownerC = anchor.web3.Keypair.generate();
    const ownerD = anchor.web3.Keypair.generate();
    const owners = [ownerA.publicKey, ownerB.publicKey, ownerC.publicKey];

    // set the threshold number of owners needed to successfully execute a tx
    const threshold = new anchor.BN(2);
    const description = "new multisig";


    // create a multisig account that will be owned by the solana program
    // we pass in:
    // 1. owners: the stakeholders of the contract(multisig wallet)
    // 2. threshhold: the minimum number of stakeholder approvals needed 
    // 3. nonce: the nonce will serve as a way for us to find the PDA needed to execute downstream wrapped program instructions
    // passing in two accounts for the context: 
    //  1. the multisig PDA and 
    //  2. the programs rent pubkey
    await program.rpc.createMultisig(description, owners, threshold, nonce, {
      accounts: {
        multisig: multisig.publicKey,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      },
      instructions: [
        await program.account.multisig.createInstruction(
          multisig,
          multisigSize
        ),
      ],
      signers: [multisig],
    });

    // run initial create multisig account tests
    let multisigAccount = await program.account.multisig.fetch(multisig.publicKey);
    assert.strictEqual(multisigAccount.nonce, nonce);
    assert.ok(multisigAccount.threshold.eq(new anchor.BN(2)));
    assert.deepStrictEqual(multisigAccount.owners, owners);
    assert.ok(multisigAccount.ownerSetSeqno === 0);


    // Grab the program id of our multisig program on the blockchain
    const pid = program.programId;

    // a list of accounts that will attach to and be a part of the specific pending tx
    // these will get passed into the instructions that are wrapped inside of the pending tx
    const accounts = [
      {
        pubkey: multisig.publicKey,
        isWritable: true,
        isSigner: false,
      },
      {
        pubkey: multisigSigner,
        isWritable: false,
        isSigner: true,
      },
    ];

    // create a list of new owners to set on the multisig wallet
    // these will get passed into the data obj which is the instruction set that will be wrapped by the pending tx
    const newOwners = [ownerA.publicKey, ownerB.publicKey, ownerD.publicKey];

    // encode the data that will ultimately be ran as instructions once the wrapping tx is approved
    // we encode the name of the isntruction, as well as the function parameters to be passed in
    const data = program.coder.instruction.encode("set_owners", {
      owners: newOwners,
    });
    // program.coder: provides a facade for encoding and decoding all IDL related objects
    // program.coder.instruction: An obj that encodes and decodes program instructions
    // program.coder.instruction.encode: Encodes a program instruction

    // create a proposed tx and attach it to the multisig account to be approved by owners
    const transaction = anchor.web3.Keypair.generate();
    const txSize = 1000; // Big enough, cuz I'm lazy.

    // create a transaction that will be default to a pending state that needs to be approved
    // Params:
    // 1. pid: We pass this in to let the transaction know this is the id of the program we're executing against
    // 2. accounts: the accounts that will be involved in the wrapped instruction 
    // 3. data: the encoded instructions that will live inside the pending transaction obj until it's ready to be executed
    // Context
    //  Accounts:
    //    1. the multisig wallet it needs to be attached to 
    //    2. the pubkey of the transaction account itself
    //    3. the pubkey of the individual who proposed the transaction
    //    4. the programs rent pubkey
    //  Instructions: A list of instructions to carry out within this transaction, here we initialize a single createInstruction to create a Transaction data account
    //  Signers: the transaction account itself and the transaction proposer
    await program.rpc.createTransaction(pid, accounts, data, {
      accounts: {
        multisig: multisig.publicKey,
        transaction: transaction.publicKey,
        proposer: ownerA.publicKey,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      },
      instructions: [
        await program.account.transaction.createInstruction(
          transaction,
          txSize
        ),
      ],
      signers: [transaction, ownerA],
    });

    const txAccount = await program.account.transaction.fetch(transaction.publicKey);

    assert.ok(txAccount.programId.equals(pid));
    assert.deepStrictEqual(txAccount.accounts, accounts);
    assert.deepStrictEqual(txAccount.data, data);
    assert.ok(txAccount.multisig.equals(multisig.publicKey));
    assert.deepStrictEqual(txAccount.didExecute, false);
    assert.ok(txAccount.ownerSetSeqno === 0);

    // Other owner approves transactoin.
    // Context
    // We send in the associated multisig account, transaction account, and the owner who is approving the transaction
    // Signers: only the approving owner needs to sign this instruction
    await program.rpc.approve({
      accounts: {
        multisig: multisig.publicKey,
        transaction: transaction.publicKey,
        owner: ownerB.publicKey,
      },
      signers: [ownerB],
    });

    // TODO: Document
    // Now that we've reached the threshold, send the transaction.
    await program.rpc.executeTransaction({
      // Set the accounts to be used in the context
      accounts: {
        multisig: multisig.publicKey,
        multisigSigner,
        transaction: transaction.publicKey,
      },
      // Set the remaining accounts that wont be intially deserialized
      remainingAccounts: program.instruction.setOwners
        .accounts({
          multisig: multisig.publicKey,
          multisigSigner,
        })
        // bc the accounts within the Auth context specified that multisig signer was a signer, we set its signer status to false bc we'll set the signer to
        // execute within the program file
        .map((meta) =>
          meta.pubkey.equals(multisigSigner)
            ? { ...meta, isSigner: false }
            : meta
        )
        // Add the program's account to the list of accounts
        .concat({
          pubkey: program.programId,
          isWritable: false,
          isSigner: false,
        }),
    });

    multisigAccount = await program.account.multisig.fetch(multisig.publicKey);

    assert.strictEqual(multisigAccount.nonce, nonce);
    assert.ok(multisigAccount.threshold.eq(new anchor.BN(2)));
    assert.deepStrictEqual(multisigAccount.owners, newOwners);
    assert.ok(multisigAccount.ownerSetSeqno === 1);
  });
});
