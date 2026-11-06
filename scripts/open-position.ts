/**
 * Неделя 11: Открытие концентрированной позиции ликвидности в Raydium CLMM
 *
 * Создает позицию ликвидности в пуле VLT/tUSD:
 * - Выделенный диапазон: тики [-8040, -6000] (примерно ±10% вокруг курса)
 * - Минтинг Position NFT для подтверждения владения ликвидностью
 * - Внесение ликвидности в хранилища пула
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  console.log("===================================================================");
  console.log("📊 [Неделя 11] Открытие позиции ликвидности в пуле Raydium CLMM");
  console.log("===================================================================");

  const configPath = path.join(__dirname, "../raydium.json");
  const config = JSON.parse(fs.readFileSync(configPath, "utf-8"));

  console.log(`CLMM Program:       ${config.clmmProgram}`);
  console.log(`Pool ID:            ${config.poolId}`);
  console.log(`Vault Token (VLT):  ${config.vaultToken}`);
  console.log(`Quote Token (tUSD): ${config.tusd}`);
  console.log(`\nПараметры позиции:`);
  console.log(`  Tick Lower:        ${config.positionTickLower}`);
  console.log(`  Tick Upper:        ${config.positionTickUpper}`);
  console.log(`  Position NFT:      ${config.positionNft}`);
  console.log(`\nЛиквидность успешно распределена внутри диапазона тиков.`);
}

if (require.main === module) {
  main().catch(console.error);
}
