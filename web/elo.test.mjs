import { test } from 'node:test'
import assert from 'node:assert/strict'

import { computeRatings, expected, parseJsonl, PROVISIONAL_GAMES, sessions, START } from './elo.js'
import { BLACK, DESTROYED, EMPTY, positions, RED, squareIndex, squareName } from './replay.js'

let clock = 0
function game(red, black, winner, extra = {}) {
    clock++
    return {
        v: 1,
        id: `g${clock}`,
        session: 's1',
        ts: `2026-10-24T12:00:${String(clock).padStart(2, '0')}.000Z`,
        red: { team: red },
        black: { team: black },
        winner,
        moves: [],
        ...extra,
    }
}

test('expected score is symmetric', () => {
    assert.equal(expected(1500, 1500), 0.5)
    assert.ok(Math.abs(expected(1600, 1400) + expected(1400, 1600) - 1) < 1e-12)
})

test('first game moves both ratings by K/2', () => {
    const [a, b] = computeRatings([game('a', 'b', 'red')])
    assert.equal(a.team, 'a')
    assert.equal(a.rating, START + 20)
    assert.equal(b.rating, START - 20)
    assert.equal(a.wins, 1)
    assert.equal(b.losses, 1)
    assert.ok(a.provisional)
})

test('anchor rating never moves', () => {
    const anchor = { team: 'ref', anchor: true }
    const g = game('a', 'x', 'black')
    g.black = anchor
    const ratings = computeRatings([g, game('ref', 'a', 'red')])
    const ref = ratings.find((r) => r.team === 'ref')
    assert.equal(ref.rating, START)
    assert.equal(ref.provisional, false)
    assert.equal(ref.games, 2)
})

test('K drops after the provisional period and resets on a new build', () => {
    const games = []
    for (let i = 0; i < PROVISIONAL_GAMES; i++) {
        const g = game('a', 'b', i % 2 ? 'red' : 'black')
        g.red.build = 'v1'
        games.push(g)
    }
    let a = computeRatings(games).find((r) => r.team === 'a')
    assert.equal(a.provisional, false)
    const next = game('a', 'b', 'red')
    next.red.build = 'v2'
    a = computeRatings([...games, next]).find((r) => r.team === 'a')
    assert.equal(a.provisional, true)
    assert.equal(a.gamesOnBuild, 1)
    assert.equal(a.games, PROVISIONAL_GAMES + 1)
})

test('games are processed in timestamp order and filtered by session', () => {
    const g1 = game('a', 'b', 'red')
    const g2 = game('a', 'b', 'black', { session: 's2' })
    const reversed = computeRatings([g2, g1])
    const ordered = computeRatings([g1, g2])
    assert.deepEqual(reversed, ordered)
    assert.equal(computeRatings([g1, g2], 's2')[0].games, 1)
    assert.deepEqual(sessions([g1, g2]), ['s2', 's1'])
})

test('parseJsonl skips truncated lines and unknown versions', () => {
    const line = JSON.stringify(game('a', 'b', 'red'))
    const { games, skipped } = parseJsonl(`${line}\n{"v":2}\n${line.slice(0, 20)}`)
    assert.equal(games.length, 1)
    assert.equal(skipped, 2)
})

test('replay squares follow server indices', () => {
    assert.equal(squareIndex('G', 1), 138)
    assert.equal(squareIndex('F', 12), 5)
    assert.equal(squareName(138), 'G1')
    const steps = positions({ opening: ['G1-G4-D7'], moves: ['F12-F9-G2'] })
    assert.equal(steps.length, 3)
    const last = steps[2].cells
    assert.equal(last[squareIndex('G', 4)], RED)
    assert.equal(last[squareIndex('F', 9)], BLACK)
    assert.equal(last[squareIndex('G', 1)], EMPTY)
    assert.equal(last[squareIndex('D', 7)], DESTROYED)
    assert.equal(last[squareIndex('G', 2)], DESTROYED)
    assert.ok(steps[1].opening && !steps[2].opening)
})
