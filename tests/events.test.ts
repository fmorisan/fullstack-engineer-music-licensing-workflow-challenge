import { before, after, describe, it } from 'node:test'
import assert from 'node:assert'
import request from 'supertest'
import { createClient } from 'redis'
import { getApp, setupDb, teardownDb, loginAs } from './helpers'
import { probeRedis, publishLicenseEvent } from '../redis'

const STUDIO_EMAIL = 'grace@gotham.studios'
const LABEL_EMAIL = 'mark@evilrecords.com'
const STUDIO_COMPANY = '849a4821-fb9a-4c6a-8fff-662cc37f6802'
const LABEL_COMPANY = 'b03a0ef7-1ff8-43aa-9085-5c3f075e9666'

let app: any
let db: any
let server: any
let listenPort: number
let studioToken: string
let labelToken: string
let redisAlive = false

before(async () => {
    db = await setupDb()
    app = await getApp()
    server = app.listen(0)
    listenPort = server.address().port
    redisAlive = await probeRedis(500)
    studioToken = await loginAs(app, STUDIO_EMAIL)
    labelToken = await loginAs(app, LABEL_EMAIL)
})

after(async () => {
    server.close()
    await teardownDb()
})

async function seedFixture(): Promise<{ movieId: string, songId: string }> {
    const movie = await request(app).post('/api/v1/movies')
        .set('Authorization', `Bearer ${studioToken}`)
        .send({ name: 'Event Reel', description: 'SSE test movie' })
    const movieId = movie.body.id

    await request(app).post(`/api/v1/movies/${movieId}/scenes`)
        .set('Authorization', `Bearer ${studioToken}`)
        .send({ name: 'Scene 1', start_offset: 0, length: 60 })

    const song = await request(app).post('/api/v1/songs')
        .set('Authorization', `Bearer ${labelToken}`)
        .send({ name: 'Signal Song', author: 'The Pubsubs', length_seconds: 90 })
    const songId = song.body.id

    return { movieId, songId }
}

async function collectEvent(channel: string, match: (event: any) => boolean, timeoutMs = 5000): Promise<any> {
    const sub = createClient({ url: process.env.REDIS_URL ?? 'redis://localhost:6379' })
    await sub.connect()

    let resolveOuter: (event: any) => void
    let rejectOuter: (err: Error) => void
    const promise = new Promise<any>((res, rej) => { resolveOuter = res; rejectOuter = rej })
    const timer = setTimeout(() => rejectOuter(new Error(`no matching event on ${channel}`)), timeoutMs)

    await sub.subscribe(channel, (message) => {
        const event = JSON.parse(message)
        if (match(event)) {
            clearTimeout(timer)
            resolveOuter(event)
        }
    })

    return promise.finally(async () => { await sub.quit() })
}

describe('license events', () => {
    it('publishes created and countered events to both company channels', async (t) => {
        if (!redisAlive) return t.skip('redis not reachable')

        const { movieId, songId } = await seedFixture()

        const studioEventPromise = collectEvent(`events:${STUDIO_COMPANY}`, (e) => e.type === 'created' && e.song_id === songId)
        const labelEventPromise = collectEvent(`events:${LABEL_COMPANY}`, (e) => e.type === 'created' && e.song_id === songId)

        const lic = await request(app).post('/api/v1/licenses')
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ movie_id: movieId, scene_number: 1, song_id: songId, start_offset: 5, length: 20, license_fee: 1234 })
        assert.equal(lic.status, 200)

        for (const p of [studioEventPromise, labelEventPromise]) {
            const event = await p
            assert.equal(event.license_id, lic.body.id)
            assert.equal(event.to_state, 'OFFER')
            assert.equal(event.license_fee, 1234)
        }

        const counterPromise = collectEvent(`events:${LABEL_COMPANY}`, (e) => e.type === 'countered')
        const patched = await request(app).patch(`/api/v1/licenses/${lic.body.id}`)
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ state: 'COUNTER', license_fee: 999 })
        assert.equal(patched.status, 200)

        const counter = await counterPromise
        assert.equal(counter.from_state, 'OFFER')
        assert.equal(counter.to_state, 'COUNTER')
        assert.equal(counter.license_fee, 999)
    })

    it('streams valid sse frames over http', async (t) => {
        if (!redisAlive) return t.skip('redis not reachable')

        const { songId } = await seedFixture()

        const controller = new AbortController()
        const res = await fetch(`http://localhost:${listenPort}/api/v1/licenses/events`, {
            headers: { Authorization: `Bearer ${studioToken}` },
            signal: controller.signal
        })

        assert.equal(res.status, 200)
        assert.equal(res.headers.get('content-type'), 'text/event-stream')
        assert.equal(res.headers.get('cache-control'), 'no-cache')

        const reader = res.body!.getReader()
        let buffer = ''

        const readUntil = async (match: (b: string) => boolean, timeoutMs = 5000) => {
            const timer = setTimeout(() => controller.abort(), timeoutMs)
            while (!match(buffer)) {
                const { value } = await reader.read()
                if (value) {
                    buffer += new TextDecoder().decode(value)
                }
            }
            clearTimeout(timer)
            return buffer
        }

        try {
            await readUntil((b) => b.startsWith('retry: 3000'))
            await readUntil((b) => b.includes('event: connected'))

            publishLicenseEvent(
                [`events:${STUDIO_COMPANY}`],
                { type: 'offered', license_id: 'sse-test', song_id: songId, from_state: 'COUNTER', to_state: 'OFFER', license_fee: 1, at: new Date().toISOString() }
            ).catch(() => {})

            const frames = await readUntil((b) => b.includes('event: license'))
            assert.ok(frames.includes(`"license_id":"sse-test"`))
            assert.ok(frames.includes('data: {'))
        } finally {
            controller.abort()
        }
    })
})
