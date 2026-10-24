// Page du ladder : lit le JSONL, calcule le classement, affiche les parties et le replay.
// Source des donnees : ?data=URL, sinon ../data/games.jsonl (serveur statique lance a la racine).

import { computeRatings, parseJsonl, sessions } from './elo.js'
import { BLACK, DESTROYED, positions, RED, SIZE } from './replay.js'

const DATA_URL = new URLSearchParams(location.search).get('data') ?? '../data/games.jsonl'
const REFRESH_MS = 15000
const RECENT_GAMES = 50
const GLOBAL = ''

const $ = (id) => document.getElementById(id)

const REASONS = {
    no_moves: 'bloqué',
    timeout: 'temps',
    disconnect: 'déconnexion',
    protocol: 'coups invalides',
    no_show: 'absent',
}

let lastText = null
let games = []
let replay = { game: null, steps: [], index: 0, timer: null }

function el(tag, attrs = {}, ...children) {
    const node = document.createElement(tag)
    for (const [k, v] of Object.entries(attrs)) {
        if (k === 'class') node.className = v
        else node.setAttribute(k, v)
    }
    node.append(...children)
    return node
}

function formatTime(ts) {
    const d = new Date(ts)
    return d.toLocaleString('fr-CA', { dateStyle: 'short', timeStyle: 'short' })
}

async function refresh() {
    try {
        const res = await fetch(DATA_URL, { cache: 'no-store' })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const text = await res.text()
        if (text !== lastText) {
            lastText = text
            const parsed = parseJsonl(text)
            games = parsed.games
            renderSessions()
            render()
            if (!replay.game && games.length) showGame(games[games.length - 1])
        }
        $('status').textContent = `${games.length} parties, mise à jour ${new Date().toLocaleTimeString('fr-CA')}`
    } catch (e) {
        $('status').textContent = `Données indisponibles (${DATA_URL}) : ${e.message}`
    }
}

function renderSessions() {
    const select = $('session')
    const current = select.value
    select.replaceChildren(
        el('option', { value: GLOBAL }, 'Toutes (global)'),
        ...sessions(games).map((s) => el('option', { value: s }, s))
    )
    select.value = [...select.options].some((o) => o.value === current) ? current : GLOBAL
}

function selectedGames() {
    const s = $('session').value
    return s === GLOBAL ? games : games.filter((g) => g.session === s)
}

function render() {
    const session = $('session').value || null
    const ratings = computeRatings(games, session)
    $('ranking')
        .querySelector('tbody')
        .replaceChildren(
            ...ratings.map((r, i) => {
                const team = el('td', {}, r.team)
                if (r.anchor) team.append(el('span', { class: 'badge anchor' }, 'ancre'))
                if (r.provisional) team.append(el('span', { class: 'badge prov' }, 'provisoire'))
                return el(
                    'tr',
                    {},
                    el('td', { class: 'num' }, String(i + 1)),
                    team,
                    el('td', { class: 'num' }, String(Math.round(r.rating))),
                    el('td', { class: 'num' }, String(r.games)),
                    el('td', { class: 'num' }, `${r.wins}-${r.losses}`),
                    el('td', { class: 'num' }, `${Math.round((100 * r.wins) / r.games)}`),
                    el('td', {}, r.last ? formatTime(r.last) : '')
                )
            })
        )

    $('anchor-note').hidden = !ratings.some((r) => r.anchor)

    const list = selectedGames()
    const times = new Set(list.map((g) => g.time_ms))
    $('time-ms').textContent = [...times].join(', ') || '?'

    $('games').replaceChildren(
        ...list
            .slice(-RECENT_GAMES)
            .reverse()
            .map((g) => {
                const side = (color) =>
                    el(
                        'span',
                        { class: `side-${color}${g.winner === color ? ' winner' : ''}` },
                        g[color].team
                    )
                const li = el(
                    'li',
                    { 'data-id': g.id, title: g.id },
                    el('time', { datetime: g.ts }, formatTime(g.ts)),
                    side('red'),
                    'contre',
                    side('black'),
                    el('span', { class: 'meta' }, `${REASONS[g.reason] ?? g.reason}, ${g.moves.length} coups`)
                )
                if (replay.game?.id === g.id) li.classList.add('selected')
                li.addEventListener('click', () => showGame(g))
                return li
            })
    )
}

function showGame(g) {
    stopPlayback()
    replay = { game: g, steps: positions(g), index: 0, timer: null }
    replay.index = replay.steps.length - 1
    $('step').max = String(replay.steps.length - 1)
    $('replay-info').textContent = `${g.red.team} (rouge) contre ${g.black.team} (noir), gagnant : ${
        g[g.winner].team
    } (${REASONS[g.reason] ?? g.reason})`
    for (const li of $('games').children) li.classList.toggle('selected', li.dataset.id === g.id)
    drawStep()
}

function drawStep() {
    const { steps, index } = replay
    const step = steps[index]
    const board = $('board')
    const nodes = []
    for (let y = 0; y < SIZE; y++) {
        nodes.push(el('div', { class: 'label' }, String(SIZE - y)))
        for (let x = 0; x < SIZE; x++) {
            const i = y * SIZE + x
            const v = step.cells[i]
            const cell = el('div', { class: 'cell' })
            if (v === DESTROYED) cell.classList.add('hole')
            if (step.move) {
                if (i === step.move.from) cell.classList.add('from')
                if (i === step.move.to) cell.classList.add('to')
                if (i === step.move.destroyed) cell.classList.add('destroyed-now')
            }
            if (v === RED) cell.append(el('div', { class: 'pawn red' }))
            if (v === BLACK) cell.append(el('div', { class: 'pawn black' }))
            nodes.push(cell)
        }
    }
    nodes.push(el('div'))
    for (let x = 0; x < SIZE; x++) nodes.push(el('div', { class: 'label' }, String.fromCharCode(65 + x)))
    board.replaceChildren(...nodes)
    $('step').value = String(index)
    const label = step.text ? `${index}/${steps.length - 1} : ${step.text}${step.opening ? ' (ouverture)' : ''}` : `0/${steps.length - 1}`
    $('step-label').textContent = label
}

function goTo(i) {
    if (!replay.game) return
    replay.index = Math.max(0, Math.min(replay.steps.length - 1, i))
    drawStep()
}

function stopPlayback() {
    clearInterval(replay.timer)
    replay.timer = null
    $('play').textContent = 'Lecture'
}

function togglePlayback() {
    if (replay.timer) return stopPlayback()
    if (replay.index === replay.steps.length - 1) goTo(0)
    $('play').textContent = 'Pause'
    replay.timer = setInterval(() => {
        if (replay.index >= replay.steps.length - 1) return stopPlayback()
        goTo(replay.index + 1)
    }, 500)
}

$('session').addEventListener('change', render)
$('first').addEventListener('click', () => goTo(0))
$('prev').addEventListener('click', () => goTo(replay.index - 1))
$('next').addEventListener('click', () => goTo(replay.index + 1))
$('last').addEventListener('click', () => goTo(replay.steps.length - 1))
$('play').addEventListener('click', togglePlayback)
$('step').addEventListener('input', (e) => goTo(+e.target.value))
document.addEventListener('keydown', (e) => {
    if (e.target.tagName === 'SELECT') return
    if (e.key === 'ArrowLeft') goTo(replay.index - 1)
    if (e.key === 'ArrowRight') goTo(replay.index + 1)
})

// Le formulaire n'apparait que si le service de soumission repond (absent en hebergement statique).
const API = '../api'

async function setupSubmit() {
    try {
        const res = await fetch(`${API}/health`, { cache: 'no-store' })
        if (!res.ok) return
    } catch {
        return
    }
    $('submit-panel').hidden = false
    $('submit-form').addEventListener('submit', async (e) => {
        e.preventDefault()
        const file = $('submit-file').files[0]
        $('submit-button').disabled = true
        $('submit-status').textContent = 'Parties de test en cours…'
        try {
            const res = await fetch(`${API}/submit`, {
                method: 'PUT',
                headers: { Authorization: `Bearer ${$('submit-token').value.trim()}` },
                body: file,
            })
            $('submit-status').textContent = (await res.text()).trim()
        } catch (err) {
            $('submit-status').textContent = `Envoi impossible : ${err.message}`
        } finally {
            $('submit-button').disabled = false
        }
    })
}

refresh()
setInterval(refresh, REFRESH_MS)
setupSubmit()
