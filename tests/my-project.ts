import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { BondingCurveNew } from "../target/types/bonding_curve_new";
import {
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Keypair,
  Transaction,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  getAssociatedTokenAddress,
  createInitializeMintInstruction,
} from "@solana/spl-token";

describe("bonding_curve_new", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.getProvider();
  const program = anchor.workspace.BondingCurveNew as Program<BondingCurveNew>;
  const wallet = provider.wallet;

  let tokenMint: PublicKey;
  let bondingCurve: PublicKey;
  let bondingCurveBump: number;

  it("Initialize bonding curve", async () => {
    tokenMint = await createMint(provider);
    [bondingCurve, bondingCurveBump] = await PublicKey.findProgramAddress(
      [Buffer.from("bonding_curve")],
      program.programId
    );

    await program.methods
      .initialize(new anchor.BN(1_000_000), new anchor.BN(100))
      .accounts({
        bondingCurve,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
        tokenMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const bondingCurveAccount = await program.account.bondingCurveState.fetch(bondingCurve);
    console.log("Bonding Curve Initialized:", bondingCurveAccount);
  });

  it("Buy tokens", async () => {
    const buyerTokenAccount = await getAssociatedTokenAddress(tokenMint, wallet.publicKey);

    await program.methods
      .buyTokens(new anchor.BN(1_000_000_000))
      .accounts({
        bondingCurve,
        buyer: wallet.publicKey,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
        tokenMint,
        buyerTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    const tokenAccount = await provider.connection.getTokenAccountBalance(buyerTokenAccount);
    console.log("Tokens Bought:", tokenAccount.value.uiAmount);
  });

  it("Sell tokens", async () => {
    const sellerTokenAccount = await getAssociatedTokenAddress(tokenMint, wallet.publicKey);

    await program.methods
      .sellTokens(new anchor.BN(500_000_000))
      .accounts({
        bondingCurve,
        seller: wallet.publicKey,
        authority: wallet.publicKey,
        systemProgram: SystemProgram.programId,
        tokenMint,
        sellerTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const tokenAccount = await provider.connection.getTokenAccountBalance(sellerTokenAccount);
    console.log("Tokens Remaining:", tokenAccount.value.uiAmount);
  });

  async function createMint(provider: anchor.AnchorProvider): Promise<PublicKey> {
    const mintKeypair = Keypair.generate();
    const rent = await provider.connection.getMinimumBalanceForRentExemption(82);

    const tx = new Transaction().add(
      SystemProgram.createAccount({
        fromPubkey: wallet.publicKey,
        newAccountPubkey: mintKeypair.publicKey,
        space: 82,
        lamports: rent,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeMintInstruction(
        mintKeypair.publicKey,
        9,
        wallet.publicKey,
        null
      )
    );

    await provider.sendAndConfirm(tx, [mintKeypair]);
    return mintKeypair.publicKey;
  }
});

