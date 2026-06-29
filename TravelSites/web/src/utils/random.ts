// 基于种子的伪随机（mulberry32 PRNG）
function mulberry32(seed: number): () => number {
  return function () {
    let t = (seed += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function dateSeed(): number {
  const d = new Date();
  return d.getFullYear() * 10000 + (d.getMonth() + 1) * 100 + d.getDate();
}

/** 用今天的日期作为种子洗牌，保证同一天结果稳定，跨天会变。 */
export function dailyShuffle<T>(arr: T[], n?: number): T[] {
  const rng = mulberry32(dateSeed());
  const copy = [...arr];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return typeof n === 'number' ? copy.slice(0, n) : copy;
}
