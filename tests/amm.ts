import * as anchor from "@coral-xyz/anchor";
import { assert, expect } from "chai";
import {
  createAccount,
  createMint,
  getAccount,
  getAssociatedTokenAddress,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Keypair, PublicKey } from "@solana/web3.js";

describe("constant product AMM", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.amm as anchor.Program;
  const authority = provider.wallet as anchor.Wallet;
  const treasury = Keypair.generate();
  const feeBps = 30;
  const initialA = 500_000;
  const initialB = 1_000_000;
  let mintA: PublicKey;
  let mintB: PublicKey;
  let userAtaA: PublicKey;
  let userAtaB: PublicKey;
  let pool: PublicKey;
  let lpMint: PublicKey;
  let vaultA: PublicKey;
  let vaultB: PublicKey;
  let treasuryAtaA: PublicKey;
  let treasuryAtaB: PublicKey;
  let treasuryAtaAKeypair: Keypair;
  let treasuryAtaBKeypair: Keypair;
  let treasuryDestinationA: PublicKey;
  let treasuryDestinationB: PublicKey;
  let userLpAta: PublicKey;
  let position: PublicKey;

  const pda = (seeds: Buffer[]) =>
    PublicKey.findProgramAddressSync(seeds, program.programId)[0];

  before(async () => {
    mintA = await createMint(
      provider.connection,
      authority.payer,
      authority.publicKey,
      null,
      6,
    );
    mintB = await createMint(
      provider.connection,
      authority.payer,
      authority.publicKey,
      null,
      6,
    );
    userAtaA = await createAccount(
      provider.connection,
      authority.payer,
      mintA,
      authority.publicKey,
      Keypair.generate(),
    );
    userAtaB = await createAccount(
      provider.connection,
      authority.payer,
      mintB,
      authority.publicKey,
      Keypair.generate(),
    );
    await mintTo(
      provider.connection,
      authority.payer,
      mintA,
      userAtaA,
      authority.payer,
      2_000_000,
    );
    await mintTo(
      provider.connection,
      authority.payer,
      mintB,
      userAtaB,
      authority.payer,
      2_000_000,
    );
    pool = pda([
      Buffer.from("pool"),
      authority.publicKey.toBuffer(),
      mintA.toBuffer(),
      mintB.toBuffer(),
    ]);
    lpMint = pda([Buffer.from("lp-mint"), pool.toBuffer()]);
    vaultA = pda([Buffer.from("vault-a"), pool.toBuffer()]);
    vaultB = pda([Buffer.from("vault-b"), pool.toBuffer()]);
    treasuryAtaAKeypair = Keypair.generate();
    treasuryAtaBKeypair = Keypair.generate();
    treasuryAtaA = await createAccount(
      provider.connection,
      authority.payer,
      mintA,
      treasury.publicKey,
      treasuryAtaAKeypair,
    );
    treasuryAtaB = await createAccount(
      provider.connection,
      authority.payer,
      mintB,
      treasury.publicKey,
      treasuryAtaBKeypair,
    );
    treasuryDestinationA = await createAccount(
      provider.connection,
      authority.payer,
      mintA,
      treasury.publicKey,
      Keypair.generate(),
    );
    treasuryDestinationB = await createAccount(
      provider.connection,
      authority.payer,
      mintB,
      treasury.publicKey,
      Keypair.generate(),
    );
    userLpAta = await getAssociatedTokenAddress(lpMint, authority.publicKey);
    position = pda([
      Buffer.from("position"),
      pool.toBuffer(),
      authority.publicKey.toBuffer(),
    ]);

    await program.methods
      .initializePool(feeBps, new anchor.BN(initialA), new anchor.BN(initialB))
      .accounts({
        authority: authority.publicKey,
        pool,
        mintA,
        mintB,
        treasury: treasury.publicKey,
        lpMint,
        vaultA,
        vaultB,
        treasuryAtaA,
        treasuryAtaB,
        authorityAtaA: userAtaA,
        authorityAtaB: userAtaB,
        authorityLpAta: userLpAta,
        position,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  });

  it("initializes a pool and mints initial LP shares", async () => {
    const lp = await getAccount(provider.connection, userLpAta);
    expect(Number(lp.amount)).to.equal(707_106);
  });

  it("adds liquidity and rejects zero deposits", async () => {
    await program.methods
      .addLiquidity(
        new anchor.BN(100_000),
        new anchor.BN(200_000),
        new anchor.BN(140_000),
      )
      .accounts({
        user: authority.publicKey,
        pool,
        mintA,
        mintB,
        userAtaA,
        userAtaB,
        vaultA,
        vaultB,
        userLpAta,
        lpMint,
        position,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    try {
      await program.methods
        .addLiquidity(new anchor.BN(0), new anchor.BN(1), new anchor.BN(0))
        .accounts({
          user: authority.publicKey,
          pool,
          mintA,
          mintB,
          userAtaA,
          userAtaB,
          vaultA,
          vaultB,
          userLpAta,
          lpMint,
          position,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();
      assert.fail("zero liquidity should fail");
    } catch (error) {
      expect(String(error)).to.contain("InvalidAmount");
    }
  });

  it("swaps in both directions and routes the fee to treasury", async () => {
    const before = await getAccount(provider.connection, treasuryAtaA);
    await program.methods
      .swap(true, new anchor.BN(10_000), new anchor.BN(1))
      .accounts({
        user: authority.publicKey,
        pool,
        mintA,
        mintB,
        vaultA,
        vaultB,
        userAtaA,
        userAtaB,
        treasuryAtaA,
        treasuryAtaB,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    const after = await getAccount(provider.connection, treasuryAtaA);
    expect(Number(after.amount) - Number(before.amount)).to.equal(30);

    await program.methods
      .swap(false, new anchor.BN(10_000), new anchor.BN(1))
      .accounts({
        user: authority.publicKey,
        pool,
        mintA,
        mintB,
        vaultA,
        vaultB,
        userAtaA,
        userAtaB,
        treasuryAtaA,
        treasuryAtaB,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  });

  it("removes liquidity and rejects an excessive minimum", async () => {
    try {
      await program.methods
        .removeLiquidity(
          new anchor.BN(1),
          new anchor.BN(1_000_000_000),
          new anchor.BN(0),
        )
        .accounts({
          user: authority.publicKey,
          pool,
          mintA,
          mintB,
          vaultA,
          vaultB,
          userAtaA,
          userAtaB,
          userLpAta,
          lpMint,
          position,
          owner: authority.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .rpc();
      assert.fail("minimum output should fail");
    } catch (error) {
      expect(String(error)).to.contain("SlippageExceeded");
    }

    await program.methods
      .removeLiquidity(
        new anchor.BN(10_000),
        new anchor.BN(1),
        new anchor.BN(1),
      )
      .accounts({
        user: authority.publicKey,
        pool,
        mintA,
        mintB,
        vaultA,
        vaultB,
        userAtaA,
        userAtaB,
        userLpAta,
        lpMint,
        position,
        owner: authority.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
  });

  it("lets only the treasury collect accumulated fees", async () => {
    await program.methods
      .collectFees(true, new anchor.BN(30))
      .accounts({
        treasury: treasury.publicKey,
        pool,
        mintA,
        mintB,
        treasuryAtaA,
        treasuryAtaB,
        treasuryDestinationA,
        treasuryDestinationB,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([treasury])
      .rpc();
    const destination = await getAccount(
      provider.connection,
      treasuryDestinationA,
    );
    expect(Number(destination.amount)).to.equal(30);
  });
});
