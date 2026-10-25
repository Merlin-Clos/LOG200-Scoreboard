// Page du ladder : lit le JSONL, calcule le classement, affiche les parties et le replay.
// Serveur : la balise meta scoreboard-server (posée par le workflow GitHub Pages), sinon le site qui
// sert la page. Données : ?data=URL, sinon <serveur>/data/games.jsonl.
// ?session=, ?team= et ?game= suivent les choix de la page : l'adresse se partage telle quelle.

import { computeRatings, parseJsonl, sessions, START } from './elo.js'
import { BLACK, DESTROYED, positions, RED, SIZE, squareName } from './replay.js'

const params = new URLSearchParams(location.search)
const SERVER = document.querySelector('meta[name="scoreboard-server"]')?.content || '..'
const DATA_URL = params.get('data') ?? `${SERVER}/data/games.jsonl`
const REFRESH_MS = 15000
const RECENT_GAMES = 50
const PLAY_MS = 500
const GLOBAL = ''
const SESSION = params.get('session') ?? GLOBAL
const TEAM = params.get('team') ?? GLOBAL
const GAME = params.get('game')

const $ = (id) => document.getElementById(id)

// Raison de la fin, du point de vue du perdant : liste compacte, puis phrase du replay.
const REASONS = {
    no_moves: 'bloqué',
    timeout: 'temps',
    disconnect: 'déconnexion',
    protocol: 'coups invalides',
    no_show: 'absent',
}
const ENDINGS = {
    no_moves: (loser) => `${loser} n'a plus de coup`,
    timeout: (loser) => `${loser} a dépassé son temps`,
    disconnect: (loser) => `${loser} s'est déconnecté`,
    protocol: (loser) => `${loser} a joué trop de coups invalides`,
    no_show: (loser) => `${loser} ne s'est pas connecté`,
}

let lastText = null
let games = []
// `pinned` : la personne a choisi une partie ; sinon le replay suit la dernière jouée.
let replay = { game: null, steps: [], index: 0, timer: null, pinned: false }

function el(tag, attrs = {}, ...children) {
    const node = document.createElement(tag)
    for (const [k, v] of Object.entries(attrs)) {
        if (k === 'class') node.className = v
        else node.setAttribute(k, v)
    }
    node.append(...children)
    return node
}

const timeFormat = new Intl.DateTimeFormat('fr-CA', { timeStyle: 'short' })
const dayFormat = new Intl.DateTimeFormat('fr-CA', { day: 'numeric', month: 'short' })
const fullFormat = new Intl.DateTimeFormat('fr-CA', { dateStyle: 'long', timeStyle: 'short' })

function sameDay(a, b) {
    return a.toDateString() === b.toDateString()
}

function shortWhen(ts) {
    const d = new Date(ts)
    return sameDay(d, new Date()) ? timeFormat.format(d) : dayFormat.format(d)
}

function ago(ts) {
    const minutes = Math.floor((Date.now() - new Date(ts)) / 60000)
    if (minutes < 1) return "à l'instant"
    if (minutes < 60) return `il y a ${minutes} min`
    if (minutes < 24 * 60) return `il y a ${Math.floor(minutes / 60)} h`
    return dayFormat.format(new Date(ts))
}

async function refresh() {
    try {
        const res = await fetch(DATA_URL, { cache: 'no-store' })
        if (!res.ok) throw new Error(`HTTP ${res.status}`)
        const text = await res.text()
        if (text !== lastText) {
            const first = lastText === null
            lastText = text
            games = parseJsonl(text).games
            renderSessions(first)
            const linked = first && games.find((g) => g.id === GAME)
            if (linked) showGame(linked, true)
            render()
            if (!first) pulse()
        } else {
            render()
        }
        $('live').className = 'live on'
    } catch (e) {
        $('live').className = 'live error'
        $('status').textContent = `Données indisponibles : ${e.message}`
    }
}

// Fondu en bas d'une zone tant qu'il reste du contenu à faire défiler.
function markOverflow(node) {
    node.classList.toggle('more', node.scrollTop + node.clientHeight < node.scrollHeight - 2)
}

const scrollAreas = [document.querySelector('.ranking-scroll'), $('games')]
for (const node of scrollAreas) node.addEventListener('scroll', () => markOverflow(node), { passive: true })
addEventListener('resize', () => scrollAreas.forEach(markOverflow))

function pulse() {
    const live = $('live')
    live.classList.remove('pulse')
    void live.offsetWidth
    live.classList.add('pulse')
}

function fillSelect(select, values, wanted = select.value) {
    select.replaceChildren(el('option', { value: GLOBAL }, 'Toutes'), ...values.map((v) => el('option', { value: v }, v)))
    select.value = values.includes(wanted) ? wanted : GLOBAL
}

function renderSessions(first) {
    fillSelect($('session'), sessions(games), first ? SESSION : $('session').value)
    const teams = [...new Set(games.flatMap((g) => [g.red.team, g.black.team]))].sort()
    fillSelect($('team'), teams, first ? TEAM : $('team').value)
}

function setParam(key, value) {
    const url = new URL(location.href)
    if (value) url.searchParams.set(key, value)
    else url.searchParams.delete(key)
    history.replaceState(null, '', url)
}

function selectedGames() {
    const s = $('session').value
    const t = $('team').value
    return games.filter(
        (g) => (s === GLOBAL || g.session === s) && (t === GLOBAL || g.red.team === t || g.black.team === t)
    )
}

function render() {
    const session = $('session').value || null
    const list = selectedGames()
    const ratings = computeRatings(games, session)
    renderRanking(ratings)
    renderChart(ratings)
    renderGames(list, new Map(ratings.flatMap((r) => r.history.map((h) => [`${h.id}|${r.team}`, h]))))

    const times = [...new Set(list.map((g) => g.time_ms))]
    $('time-ms').textContent = times.join(', ') || '?'
    const pace = times.length === 1 ? `, ${times[0]} ms par coup` : ''
    $('status').textContent = `${list.length} parties${pace}, à jour à ${timeFormat.format(new Date())}`

    scrollAreas.forEach(markOverflow)

    const latest = list.at(-1)
    if (latest && !replay.pinned && replay.game?.id !== latest.id) showGame(latest)
}

function renderRanking(ratings) {
    $('ranking-empty').hidden = ratings.length > 0
    $('anchor-note').hidden = !ratings.some((r) => r.anchor)
    $('ranking')
        .querySelector('tbody')
        .replaceChildren(
            ...ratings.map((r, i) => {
                const team = el('td', { class: 'team', title: r.team }, r.team)
                if (r.anchor) team.append(el('span', { class: 'badge anchor' }, 'ancre'))
                if (r.provisional) team.append(el('span', { class: 'badge' }, 'provisoire'))
                return el(
                    'tr',
                    {},
                    el('td', { class: 'num rank' }, String(i + 1)),
                    team,
                    el('td', { class: 'num elo' }, String(Math.round(r.rating))),
                    el('td', { class: 'num record' }, `${r.wins}–${r.losses}`),
                    el('td', { class: 'num pct' }, `${Math.round((100 * r.wins) / r.games)} %`),
                    el(
                        'td',
                        { class: 'num last-col', title: r.last ? fullFormat.format(new Date(r.last)) : '' },
                        r.last ? ago(r.last) : ''
                    )
                )
            })
        )
}

function renderChart(ratings) {
    const team = $('team').value
    const r = ratings.find((r) => r.team === team)
    $('elo-chart').hidden = !r
    if (!r) return
    const values = [START, ...r.history.map((h) => h.rating)]
    const min = Math.min(...values)
    const max = Math.max(...values)
    const now = Math.round(values.at(-1))
    const svg = $('elo-line')
    const line = document.createElementNS(svg.namespaceURI, 'polyline')
    line.setAttribute('points', values.map((v, i) => `${i},${max - v}`).join(' '))
    svg.setAttribute('viewBox', `0 0 ${values.length - 1} ${max - min || 1}`)
    svg.setAttribute('aria-label', `Elo de ${team} : de ${START} à ${now} sur ${r.history.length} parties`)
    svg.replaceChildren(line)
    $('elo-caption').textContent = `${team} : ${now} Elo, entre ${Math.round(min)} et ${Math.round(max)}`
}

const signed = (n) => `${n > 0 ? '+' : '−'}${Math.abs(n)}`

// `deltas` : clé `${id}|${équipe}` vers l'entrée d'historique Elo de cette partie.
function renderGames(list, deltas) {
    const items = list
        .slice(-RECENT_GAMES)
        .reverse()
        .map((g) => {
            const entry = (color) => deltas.get(`${g.id}|${g[color].team}`)
            const change = (color) => Math.round(entry(color)?.delta ?? 0)
            const side = (color) => {
                const node = el(
                    'span',
                    { class: `side ${color}${g.winner === color ? ' winner' : ''}` },
                    el('span', {}, g[color].team)
                )
                const d = change(color)
                if (d) {
                    const title = `Elo après la partie : ${Math.round(entry(color).rating)}`
                    node.append(el('span', { class: `delta ${d > 0 ? 'up' : 'down'}`, title }, signed(d)))
                }
                return node
            }
            const who = (color, name) => `${g[color].team} (${name}${change(color) ? `, ${signed(change(color))}` : ''})`
            const button = el(
                'button',
                {
                    type: 'button',
                    class: 'game',
                    'data-id': g.id,
                    'aria-label': `${who('red', 'rouge')} contre ${who('black', 'noir')}, ${g[g.winner].team} gagne`,
                },
                el('time', { datetime: g.ts, title: fullFormat.format(new Date(g.ts)) }, shortWhen(g.ts)),
                side('red'),
                side('black'),
                el('span', { class: 'meta' }, `${REASONS[g.reason] ?? g.reason}, ${g.moves.length} coups`)
            )
            if (replay.game?.id === g.id) button.setAttribute('aria-current', 'true')
            button.addEventListener('click', () => showGame(g, true))
            return el('li', {}, button)
        })
    $('games').replaceChildren(
        ...(items.length ? items : [el('li', { class: 'empty' }, 'Les parties jouées apparaîtront ici.')])
    )
}

function showGame(g, pinned = replay.pinned) {
    stopPlayback()
    const steps = positions(g)
    replay = { game: g, steps, index: steps.length - 1, timer: null, pinned }
    setParam('game', pinned ? g.id : null)
    $('step').max = String(steps.length - 1)

    const loser = g.winner === 'red' ? 'black' : 'red'
    const ending = ENDINGS[g.reason]?.(g[loser].team) ?? g.reason
    $('replay-info').replaceChildren(
        el('strong', {}, g.red.team),
        ' (rouge) contre ',
        el('strong', {}, g.black.team),
        ` (noir) : ${g[g.winner].team} gagne, ${ending}.`
    )
    for (const button of $('games').querySelectorAll('.game')) {
        if (button.dataset.id === g.id) button.setAttribute('aria-current', 'true')
        else button.removeAttribute('aria-current')
    }
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

    // Les rouges jouent les coups impairs : 1, 3, 5...
    board.dataset.mover = index % 2 === 1 ? 'red' : 'black'
    const total = steps.length - 1
    $('step').value = String(index)
    if (step.move) {
        const { from, to, destroyed } = step.move
        $('move-count').textContent = `Coup ${index} sur ${total}${step.opening ? ', ouverture' : ''}`
        $('move-text').replaceChildren(
            `${squareName(from)} → ${squareName(to)} `,
            el('span', { class: 'destroyed' }, `détruit ${squareName(destroyed)}`)
        )
        board.setAttribute(
            'aria-label',
            `Coup ${index} sur ${total} : ${squareName(from)} vers ${squareName(to)}, ${squareName(destroyed)} détruite`
        )
    } else {
        $('move-count').textContent = `Position initiale, ${total} coups`
        $('move-text').replaceChildren()
        board.setAttribute('aria-label', 'Plateau 12 × 12, position initiale')
    }
}

function goTo(i) {
    if (!replay.game) return
    replay.index = Math.max(0, Math.min(replay.steps.length - 1, i))
    drawStep()
}

function stopPlayback() {
    clearInterval(replay.timer)
    replay.timer = null
    $('play').classList.remove('playing')
    $('play').setAttribute('aria-label', 'Lecture')
}

function togglePlayback() {
    if (!replay.game) return
    if (replay.timer) return stopPlayback()
    replay.pinned = true
    if (replay.index === replay.steps.length - 1) goTo(0)
    $('play').classList.add('playing')
    $('play').setAttribute('aria-label', 'Pause')
    replay.timer = setInterval(() => {
        if (replay.index >= replay.steps.length - 1) return stopPlayback()
        goTo(replay.index + 1)
    }, PLAY_MS)
}

// Parcourir une partie la fige : le replay ne saute plus vers la dernière jouée.
function browse(i) {
    replay.pinned = true
    setParam('game', replay.game?.id)
    goTo(i)
}

async function share() {
    if (!replay.game) return
    replay.pinned = true
    setParam('game', replay.game.id)
    try {
        await navigator.clipboard.writeText(location.href)
        $('share').textContent = 'Lien copié'
    } catch {
        $('share').textContent = "Copiez l'adresse de la page"
    }
    setTimeout(() => ($('share').textContent = 'Copier le lien'), 2000)
}

for (const id of ['session', 'team']) {
    $(id).addEventListener('change', () => {
        setParam(id, $(id).value)
        replay.pinned = false
        render()
    })
}
$('share').addEventListener('click', share)
$('first').addEventListener('click', () => browse(0))
$('prev').addEventListener('click', () => browse(replay.index - 1))
$('next').addEventListener('click', () => browse(replay.index + 1))
$('last').addEventListener('click', () => browse(replay.steps.length - 1))
$('play').addEventListener('click', togglePlayback)
$('step').addEventListener('input', (e) => browse(+e.target.value))
document.addEventListener('keydown', (e) => {
    if (e.target.closest('input, select, textarea, [contenteditable]')) return
    if (e.key === 'ArrowLeft') browse(replay.index - 1)
    if (e.key === 'ArrowRight') browse(replay.index + 1)
})

// Le formulaire n'apparaît que si le service de soumission répond (absent en hébergement statique).
const API = `${SERVER}/api`

async function setupSubmit() {
    try {
        const res = await fetch(`${API}/health`, { cache: 'no-store' })
        if (!res.ok) return
    } catch {
        return
    }
    $('submit-panel').hidden = false
    const status = $('submit-status')
    $('submit-file').addEventListener('change', (e) => {
        $('submit-file-name').textContent = e.target.files[0]?.name ?? 'Choisir le jar'
    })
    $('submit-form').addEventListener('submit', async (e) => {
        e.preventDefault()
        const file = $('submit-file').files[0]
        $('submit-button').disabled = true
        status.dataset.state = 'pending'
        status.textContent = 'Parties de test en cours, jusqu’à deux minutes…'
        try {
            const res = await fetch(`${API}/submit`, {
                method: 'PUT',
                headers: { Authorization: `Bearer ${$('submit-token').value.trim()}` },
                body: file,
            })
            status.dataset.state = res.ok ? 'ok' : 'error'
            status.textContent = (await res.text()).trim()
        } catch (err) {
            status.dataset.state = 'error'
            status.textContent = `Envoi impossible : ${err.message}. Vérifiez la connexion, puis réessayez.`
        } finally {
            $('submit-button').disabled = false
        }
    })
}

refresh()
setInterval(refresh, REFRESH_MS)
setupSubmit()
