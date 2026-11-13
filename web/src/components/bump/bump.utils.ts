import { smooth, type TRankPoint } from '@/lib/bump';
import { dayFormat, timeFormat } from '@/lib/format';
import type { TRatingRow } from '@/types/games.types';

export const MARGIN = { top: 18, right: 96, bottom: 28, left: 36 };
// Pixels par partie en dessous desquels le rang est lissé, et largeur d'un paquet lissé.
export const EXACT_PX = 4;
export const BUCKET_PX = 12;
// Couleurs --series-1 à --series-10, répétées au-delà.
const COLORS = 10;

export type TVertex = [number, number];
export type TChartMetric = 'rank' | 'rating';

// Équipe vers son emplacement de couleur, pour toute la page : une équipe garde sa couleur.
const colors = new Map<string, number>();

// Première couleur libre ; les équipes retirées libèrent la leur.
export function colorOf(team: string, selected: string[]) {
  for (const t of colors.keys()) if (!selected.includes(t)) colors.delete(t);
  if (!colors.has(team)) {
    const used = new Set(colors.values());
    let slot = 0;
    while (used.has(slot) && slot < COLORS) slot++;
    colors.set(team, slot < COLORS ? slot : colors.size % COLORS);
  }
  return `var(--series-${colors.get(team)! + 1})`;
}

// Sommets [colonne, rang ou Elo] de la ligne. Valeur exacte : palier, puis diagonale sur une partie.
export function vertices(points: TRankPoint[], count: number, buckets: number, metric: TChartMetric) {
  const line = metric === 'rank' ? smooth(points, count, buckets) : points;
  const path: TVertex[] = [];
  if (line === points) {
    points.forEach((p, i) => {
      if (i) path.push([Math.max(p.x - 1, points[i - 1].x), points[i - 1][metric]]);
      path.push([p.x, p[metric]]);
    });
  } else {
    path.push([points[0].x, line[0].rank], ...line.map((p): TVertex => [p.x, p.rank]));
  }
  path.push([count - 1, path[path.length - 1][1]]);
  return path;
}

// Rang ou Elo interpolé de la ligne à `col` : un nœud se pose sur la ligne, même lissée.
export function valueAt(path: TVertex[], col: number) {
  const k = path.findIndex(([c]) => c >= col);
  if (k <= 0) return path[Math.max(k, 0)][1];
  const [[c0, r0], [c1, r1]] = [path[k - 1], path[k]];
  return c1 === c0 ? r1 : r0 + ((r1 - r0) * (col - c0)) / (c1 - c0);
}

// Cinq dates au plus, une par 100 px.
export function dateTicks(columns: TRatingRow[], width: number) {
  const span = Date.parse(columns[columns.length - 1].ts) - Date.parse(columns[0].ts);
  const format = span < 864e5 ? timeFormat : dayFormat;
  const count = Math.min(5, columns.length, Math.max(2, Math.floor(width / 100)));
  const ticks: { col: number; text: string; anchor: 'start' | 'middle' | 'end' }[] = [];
  let previous = '';
  for (let k = 0; k < count; k++) {
    const col = count > 1 ? Math.round((k * (columns.length - 1)) / (count - 1)) : 0;
    const text = format.format(new Date(columns[col].ts));
    if (text === previous) continue;
    previous = text;
    ticks.push({ col, text, anchor: k === 0 ? 'start' : k === count - 1 ? 'end' : 'middle' });
  }
  return ticks;
}
