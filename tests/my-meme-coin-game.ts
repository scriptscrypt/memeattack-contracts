import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { MemeCoinGame } from "../target/types/meme_coin_game";
import { expect, assert } from "chai";


describe("my-meme-coin-game", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.MemeCoinGame as Program<MemeCoinGame>;

  const gameId = anchor.web3.Keypair.generate();
  const initialPrizePerBox = new anchor.BN(1_000_000); // 0.001 SOL per box
  const totalPrize = initialPrizePerBox.mul(new anchor.BN(9)); // 9 boxes
  const player1 = anchor.web3.Keypair.generate();
  const player2 = anchor.web3.Keypair.generate();

   // Derive PDAs
   const [gameStatePda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("game-state"), gameId.publicKey.toBuffer()],
    program.programId
  );

  async function airdropSol(connection: anchor.web3.Connection, address: anchor.web3.PublicKey, amount: number) {
    const airdropSignature = await connection.requestAirdrop(address, amount);
    await connection.confirmTransaction(airdropSignature);
  }

  before(async () => {
    const airdropAmount = 2 * anchor.web3.LAMPORTS_PER_SOL; // 2 SOL
    await airdropSol(provider.connection, player1.publicKey, airdropAmount);
    await airdropSol(provider.connection, player2.publicKey, airdropAmount);

  });

 
  it("Initializes the game", async () => {
    const tx = await program.methods
      .initialize(initialPrizePerBox)
      .accounts({
        gameState: gameStatePda,
        gameId: gameId.publicKey,
        user: provider.wallet.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      //.signers([provider.wallet.publicKey])
      .rpc();

    const gameState = await program.account.gameState.fetch(gameStatePda);
    // Assert game_id
    assert.ok(gameState.gameId.equals(gameId.publicKey), "Game ID doesn't match");

    // Assert prize pool
    assert.ok(gameState.prizePool.eq(totalPrize), "Prize pool doesn't match");

    // Existing assertions
    assert.equal(gameState.boxes.length, 9);
    gameState.boxes.forEach((box) => {
      assert.equal(box.memeCoinName, "");
      assert.ok(box.amountInLamports.eq(initialPrizePerBox));
      assert.equal(box.startTime.toNumber(), 0);
      assert.equal(box.contributions.length, 0);
    });
    console.log("Game initialized with transaction signature", tx);
  });


  it("Can enter the game", async () => {
    const boxNumber = 0;
    const memeCoinName = "DOGE";
    const amountInLamports = new anchor.BN(1_000_000_000); // 1 SOL

    await program.methods.enterGame(memeCoinName, amountInLamports, boxNumber)
      .accounts({
        gameState: gameStatePda,
        player: player1.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    const updatedGameState = await program.account.gameState.fetch(gameStatePda);
    const updatedBox = updatedGameState.boxes[boxNumber];

    // console.log("box amount", Number(updatedBox.amountInLamports));
    // console.log("amount in lamports",Number(amountInLamports));

    assert.equal(updatedBox.memeCoinName, memeCoinName, "Meme coin name doesn't match");
    assert.ok(updatedBox.amountInLamports.eq(amountInLamports.add(initialPrizePerBox)), "Box amount doesn't match");
    assert.equal(updatedBox.contributions.length, 1, "Contributions array length is incorrect");
    assert.ok(updatedBox.contributions[0].contributor.equals(player1.publicKey), "Contributor doesn't match");
    assert.ok(updatedBox.contributions[0].amount.eq(amountInLamports), "Contribution amount doesn't match");
    assert.ok(updatedGameState.prizePool.eq(initialPrizePerBox.mul(new anchor.BN(9)).add(amountInLamports)), "Prize pool doesn't match");
  });

  it("Can claim prize", async () => {
    const boxNumber = 0;
    const memeCoinName = "DOGE";
    const amountInLamports = new anchor.BN(1_000_000_000); // 1 SOL

    // First, enter the game
    await program.methods.enterGame(memeCoinName, amountInLamports, boxNumber)
      .accounts({
        gameState: gameStatePda,
        player: player1.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([player1])
      .rpc();

    // Fast-forward time by 1 hour (3600 seconds)
    // I disabled the time checking in the program for testing purposes

    const initialBalance = await provider.connection.getBalance(player1.publicKey);

    // Now claim the prize
    await program.methods.claimPrize(boxNumber, memeCoinName)
      .accounts({
        gameState: gameStatePda,
        player: player1.publicKey,
      })
      .signers([player1])
      .rpc();

    const finalBalance = await provider.connection.getBalance(player1.publicKey);
    const updatedGameState = await program.account.gameState.fetch(gameStatePda);
    const updatedBox = updatedGameState.boxes[boxNumber];

    // Assert the player received the prize
    assert.ok(finalBalance > initialBalance, "Player balance didn't increase");

    // Assert the box is reset
    assert.equal(updatedBox.memeCoinName, "", "Box meme coin name not reset");
    assert.ok(updatedBox.amountInLamports.eq(new anchor.BN(0)), "Box amount not reset");
    assert.equal(updatedBox.contributions.length, 0, "Box contributions not cleared");

    // Assert the prize pool is reduced
    assert.ok(updatedGameState.prizePool.lt(amountInLamports.add(initialPrizePerBox)), "Prize pool not reduced");
  });

});
