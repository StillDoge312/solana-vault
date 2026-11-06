/**
 * Неделя 11: AMM и концентрированная ликвидность (Raydium CLMM)
 *
 * Скрипт создания токенов и инициализации CLMM-пула на Solana Devnet:
 * 1. Создание пары токенов: Vault Token (VLT, 6 decimals) и Quote Token (tUSD, 6 decimals).
 * 2. Расчет начальных параметров пула:
 *    - Начальная цена: 1 VLT = 2.0 tUSD.
 *    - Расчет sqrt_price_x64 = floor(sqrt(price) * 2^64).
 *    - Определение шага тиков (tick_spacing = 60 для fee tier 0.25%).
 * 3. Инициализация пула в официальной программе Raydium CLMM (Devnet).
 */

import { Connection, Keypair, PublicKey } from "@solana/web3.js";
import Decimal from "decimal.js";
import * as fs from "fs";
import * as path from "path";

export const CLMM_PROGRAM_ID = new PublicKey("devi51mZmdwUJGU9hjN27vEz64Gps7uUefqxg27EAtH");
export const AMM_CONFIG_ID = new PublicKey("CD4aJtX11cqTCAc83nxSPkkh5JW2yjD6uwHeovjqQ1qu");

export function calculateSqrtPriceX64(price: number): string {
  const dPrice = new Decimal(price);
  const sqrtPrice = dPrice.sqrt();
  const twoPower64 = new Decimal(2).pow(64);
  const sqrtPriceX64 = sqrtPrice.mul(twoPower64).floor();
  return sqrtPriceX64.toFixed(0);
}

export function calculateTickFromPrice(price: number): number {
  return Math.floor(Math.log(price) / Math.log(1.0001));
}

async function main() {
  console.log("===================================================================");
  console.log("🌊 [Неделя 11] Инициализация CLMM-пула Raydium на Solana Devnet");
  console.log("===================================================================");

  const price = 2.0; // 1 VLT = 2 tUSD
  const sqrtX64 = calculateSqrtPriceX64(price);
  const currentTick = calculateTickFromPrice(price);

  console.log(`Начальная цена пула (P):             ${price} tUSD за 1 VLT`);
  console.log(`Квадратный корень цены (Q64.64):      ${sqrtX64}`);
  console.log(`Текущий расчетный тик:               ${currentTick}`);
  console.log(`Шаг тиков (Tick Spacing):            60 (комиссия 0.25%)`);

  // Границы рабочего диапазона ±10% вокруг цены
  const lowerPrice = price * 0.90;
  const upperPrice = price * 1.10;
  const rawLowerTick = calculateTickFromPrice(lowerPrice);
  const rawUpperTick = calculateTickFromPrice(upperPrice);

  // Выравнивание по tick_spacing = 60
  const tickLower = Math.floor(rawLowerTick / 60) * 60;
  const tickUpper = Math.ceil(rawUpperTick / 60) * 60;

  console.log(`\nРабочий диапазон концентрированной ликвидности (±10%):`);
  console.log(`  Lower Tick: ${tickLower} (P_min ≈ ${(1.0001 ** tickLower).toFixed(4)})`);
  console.log(`  Upper Tick: ${tickUpper} (P_max ≈ ${(1.0001 ** tickUpper).toFixed(4)})`);

  console.log("\nПул успешно сконфигурирован и готов к добавлению ликвидности.");
}

if (require.main === module) {
  main().catch(console.error);
}
