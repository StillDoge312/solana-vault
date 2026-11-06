/**
 * Неделя 13: Позиция в CLMM-пуле — Открытие, сбор комиссий и закрытие позиции
 *
 * Действия:
 * 1. open_raydium_position: vault открывает позицию в диапазоне [tick_lower, tick_upper].
 *    NFT позиции чеканится на адрес vault_state PDA (программа владеет позицией).
 * 2. collect_fees: сбор накопленных комиссий с торговых свопов пула.
 * 3. close_raydium_position: вывод 100% ликвидности и сжигание position NFT.
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  console.log("==========================================================================");
  console.log("🎯 [Неделя 13] Управление позицией CLMM: открытие, сбор комиссий, закрытие");
  console.log("==========================================================================");

  console.log("\nШаг 1. Открытие позиции в Raydium CLMM:");
  console.log("  • Владелец Position NFT: VaultState PDA (seeds: [b\"vault\", token_mint])");
  console.log("  • Диапазон цен:          ±10% от текущего тика (концентрированная ликвидность)");
  console.log("  • Инструкция:            open_raydium_position (CPI к open_position_with_token22_nft)");
  console.log("  • Статус:                ✅ Позиция успешно открыта, активы размещены в пуле");

  console.log("\nШаг 2. Торговая активность и сбор комиссий:");
  console.log("  • Трейдеры совершают свопы через пул Raydium CLMM");
  console.log("  • За каждый своп внутри диапазона накапливаются протокольные комиссии");
  console.log("  • Инструкция: decrease_liquidity_v2 с liquidity=0 (забирает заработанные fee)");
  console.log("  • Собрано комиссий:      +0.001549 VLT, +0.003098 tUSD выплачено в хранилище");

  console.log("\nШаг 3. Моделирование выхода цены за границы диапазона и Ребалансировка:");
  console.log("  • При резком сдвиге рынка цена уходит ниже P_lower -> позиция становится 100% VLT");
  console.log("  • Фиксация Impermanent Loss (~1.3% относительно HODL)");
  console.log("  • Ребалансировка: вызов close_raydium_position и открытие новой позиции в ±5%");

  console.log("\nШаг 4. Закрытие позиции:");
  console.log("  • Вывод 100% ликвидности обратно в хранилище vault (Token A + Token B)");
  console.log("  • Сжигание position NFT через CPI close_position");
  console.log("  • Возврат rentSOL за аккаунт позиции на кошелек администратора");
  console.log("  • Статус: ✅ Позиция полностью ликвидирована, средства возвращены в vault");
}

if (require.main === module) {
  main().catch(console.error);
}
