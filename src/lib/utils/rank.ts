export interface RankInfo {
  tierName: 'Bronze' | 'Silver' | 'Gold' | 'Platinum' | 'Diamond' | 'Obsidian' | 'Mythic';
  level: number;
  exp: number;
  levelMinExp: number;
  nextLevelExp: number;
  progressPercent: number;
  icon: string;
  badgeStyle: string;
  barColor: string;
}

interface TierDef {
  name: RankInfo['tierName'];
  minExp: number;
  maxExp: number;
  minLevel: number;
  maxLevel: number;
  icon: string;
  badgeStyle: string;
  barColor: string;
}

const TIERS: TierDef[] = [
  {
    name: 'Bronze',
    minExp: 0,
    maxExp: 1000,
    minLevel: 1,
    maxLevel: 5,
    icon: '🥉',
    badgeStyle: 'background: rgba(180, 110, 50, 0.16); color: #e2a46b;',
    barColor: '#cd7f32',
  },
  {
    name: 'Silver',
    minExp: 1000,
    maxExp: 5000,
    minLevel: 6,
    maxLevel: 10,
    icon: '🥈',
    badgeStyle: 'background: rgba(148, 163, 184, 0.16); color: #cbd5e1;',
    barColor: '#94a3b8',
  },
  {
    name: 'Gold',
    minExp: 5000,
    maxExp: 20000,
    minLevel: 11,
    maxLevel: 20,
    icon: '🥇',
    badgeStyle: 'background: rgba(234, 179, 8, 0.16); color: #fde047;',
    barColor: '#eab308',
  },
  {
    name: 'Platinum',
    minExp: 20000,
    maxExp: 60000,
    minLevel: 21,
    maxLevel: 35,
    icon: '💠',
    badgeStyle: 'background: rgba(56, 189, 248, 0.16); color: #7dd3fc;',
    barColor: '#38bdf8',
  },
  {
    name: 'Diamond',
    minExp: 60000,
    maxExp: 150000,
    minLevel: 36,
    maxLevel: 50,
    icon: '💎',
    badgeStyle: 'background: rgba(0, 120, 232, 0.2); color: #6cb4ff;',
    barColor: '#0078e8',
  },
  {
    name: 'Obsidian',
    minExp: 150000,
    maxExp: 400000,
    minLevel: 51,
    maxLevel: 75,
    icon: '🔮',
    badgeStyle: 'background: rgba(139, 92, 246, 0.2); color: #c4b5fd;',
    barColor: '#8b5cf6',
  },
  {
    name: 'Mythic',
    minExp: 400000,
    maxExp: 1000000,
    minLevel: 76,
    maxLevel: 100,
    icon: '👑',
    badgeStyle: 'background: rgba(244, 63, 94, 0.2); color: #fda4af;',
    barColor: '#f43f5e',
  },
];

export function getRankInfo(rawExp: number = 0): RankInfo {
  const exp = Math.max(0, Math.floor(rawExp || 0));
  const tier =
    [...TIERS].reverse().find((t) => exp >= t.minExp) || TIERS[0];

  const levelCount = tier.maxLevel - tier.minLevel + 1;
  const span = Math.max(1, tier.maxExp - tier.minExp);
  const perLevel = span / levelCount;

  const relExp = Math.max(0, exp - tier.minExp);
  const offset = Math.min(levelCount - 1, Math.floor(relExp / perLevel));
  const level =
    exp >= 1_000_000 ? 100 + Math.floor((exp - 1_000_000) / 50_000) : tier.minLevel + offset;

  const levelMinExp = Math.floor(tier.minExp + offset * perLevel);
  const nextLevelExp = Math.floor(tier.minExp + (offset + 1) * perLevel);
  const levelSpan = Math.max(1, nextLevelExp - levelMinExp);
  const progressPercent = Math.min(
    100,
    Math.max(2, Math.round(((exp - levelMinExp) / levelSpan) * 100))
  );

  return {
    tierName: tier.name,
    level,
    exp,
    levelMinExp,
    nextLevelExp,
    progressPercent,
    icon: tier.icon,
    badgeStyle: tier.badgeStyle,
    barColor: tier.barColor,
  };
}
