// Classement Elo calcule a partir du JSONL du ladder (format v1, voir docs/formats.md).

export const START = 1500
export const K_PROVISIONAL = 40
export const K_STABLE = 20
// Nombre de parties a K provisoire apres chaque nouvelle soumission (nouveau hash de build).
export const PROVISIONAL_GAMES = 20

// Lignes illisibles (dont une derniere ligne en cours d'ecriture) et versions inconnues ignorees.
export function parseJsonl(text) {
    const games = []
    let skipped = 0
    for (const line of text.split('\n')) {
        if (!line.trim()) continue
        try {
            const g = JSON.parse(line)
            if (g.v === 1) games.push(g)
            else skipped++
        } catch {
            skipped++
        }
    }
    return { games, skipped }
}

export function expected(ra, rb) {
    return 1 / (1 + 10 ** ((rb - ra) / 400))
}

function entry(stats, player) {
    let s = stats.get(player.team)
    if (!s) {
        s = {
            team: player.team,
            rating: START,
            games: 0,
            wins: 0,
            losses: 0,
            anchor: false,
            build: null,
            gamesOnBuild: 0,
            last: null,
            history: [],
        }
        stats.set(player.team, s)
    }
    if (player.anchor) s.anchor = true
    // Nouvelle soumission : le rating est conserve, K repasse en provisoire.
    if (player.build && player.build !== s.build) {
        s.build = player.build
        s.gamesOnBuild = 0
    }
    return s
}

// Les parties sont rejouees dans l'ordre des horodatages. `session` restreint le calcul a une session.
export function computeRatings(games, session = null) {
    const stats = new Map()
    const sorted = games
        .filter((g) => session === null || g.session === session)
        .slice()
        .sort((a, b) => (a.ts < b.ts ? -1 : a.ts > b.ts ? 1 : 0))
    for (const g of sorted) {
        const red = entry(stats, g.red)
        const black = entry(stats, g.black)
        const scoreRed = g.winner === 'red' ? 1 : 0
        const expRed = expected(red.rating, black.rating)
        const deltas = [
            [red, scoreRed - expRed],
            [black, 1 - scoreRed - (1 - expRed)],
        ]
        for (const [s, diff] of deltas) {
            const k = s.gamesOnBuild < PROVISIONAL_GAMES ? K_PROVISIONAL : K_STABLE
            if (!s.anchor) s.rating += k * diff
            s.games++
            s.gamesOnBuild++
            if (diff > 0) s.wins++
            else s.losses++
            s.last = g.ts
            s.history.push({ ts: g.ts, rating: s.rating })
        }
    }
    return [...stats.values()]
        .map((s) => ({
            ...s,
            provisional: !s.anchor && s.gamesOnBuild < PROVISIONAL_GAMES,
        }))
        .sort((a, b) => b.rating - a.rating)
}

export function sessions(games) {
    const latest = new Map()
    for (const g of games) {
        if (!latest.has(g.session) || latest.get(g.session) < g.ts) latest.set(g.session, g.ts)
    }
    return [...latest.entries()].sort((a, b) => (a[1] < b[1] ? 1 : -1)).map(([s]) => s)
}
