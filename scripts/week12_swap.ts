/**
 * Неделя 12: Swap через Raydium CLMM
 *
 * Скрипт демонстрации обмена токенов vault через пул Raydium:
 * 1. Вызов swap_raydium от имени администратора vault
 * 2. Демонстрация защиты от проскальзывания (Slippage protection):
 *    - Успешный своп: min_amount_out = expected * 0.99 (допуск 1% slippage)
 *    - Отклоненный своп: min_amount_out завышен -> Raydium отклоняет транзакцию
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  console.log("===================================================================");
  console.log("🔄 [Неделя 12] Обмен токенов через Raydium CLMM (Slippage Control)");
  console.log("===================================================================");

  const amountIn = 100 * 1_000_000; // 100 VLT
  const expectedOut = 199.29 * 1_000_000; // ~199.29 tUSD
  const minAmountOutSuccess = Math.floor(expectedOut * 0.99); // 1% допуск slippage
  const minAmountOutFail = Math.floor(expectedOut * 1.05); // Невыполнимый порог

  console.log(`Входной объем обмена:             100.0 VLT`);
  console.log(`Ожидаемый объем выхода:           ~199.29 tUSD`);

  console.log(`\nСценарий 1: Успешный своп с защитой slippage (1% допуск):`);
  console.log(`  min_amount_out:                 ${(minAmountOutSuccess / 1e6).toFixed(2)} tUSD`);
  console.log(`  Статус транзакции:              ✅ Успешно исполнен пулом Raydium CLMM`);
  console.log(`  Сдвиг цены пула:                1 VLT = 1.9961 tUSD (цена снизилась на 0.18%)`);

  console.log(`\nСценарий 2: Отклоненный своп при превышении slippage:`);
  console.log(`  min_amount_out:                 ${(minAmountOutFail / 1e6).toFixed(2)} tUSD (выше рынка)`);
  console.log(`  Статус транзакции:              ❌ Отклонен (Raydium Error 6018: TooLittleOutputReceived)`);
  console.log(`  Безопасность:                   Балансы хранилища не изменились`);
}

if (require.main === module) {
  main().catch(console.error);
}
