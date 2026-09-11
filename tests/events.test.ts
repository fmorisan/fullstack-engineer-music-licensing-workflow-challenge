import { before, after, describe, it } from 'node:test'
import assert from 'node:assert'
import request from 'supertest'
import { createClient, type RedisClientType } from 'redis'
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
let studioSub: RedisClientType | null = null
let labelSub: RedisClientType | null = null
let studioEvents: any[] = []
let labelEvents: any[] = []

before(async () => {
    db = await setupDb()
    app = await getApp()
    server = app.listen(0)
    listenPort = server.address().port
    redisAlive = await probeRedis(500)
    studioToken = await loginAs(app, STUDIO_EMAIL)
    labelToken = await loginAs(app, LABEL_EMAIL)

    if (redisAlive) {
        const url = process.env.REDIS_URL ?? 'redis://localhost:6379'
        studioSub = createClient({ url })
        labelSub = createClient({ url })
        await studioSub.connect()
        await labelSub.connect()
        await studioSub.subscribe(`events:${STUDIO_COMPANY}`, (message) => { studioEvents.push(JSON.parse(message)) })
        await labelSub.subscribe(`events:${LABEL_COMPANY}`, (message) => { labelEvents.push(JSON.parse(message)) })
    }
})

after(async () => {
    if (studioSub) await studioSub.quit().catch(() => {})
    if (labelSub) await labelSub.quit().catch(() => {})
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

function collectFrom(buffer: any[], match: (event: any) => boolean, label: string, timeoutMs = 5000): Promise<any> {
    return new Promise((resolve, reject) => {
        const existing = buffer.find(match)
        if (existing) return resolve(existing)
        const timer = setTimeout(() => reject(new Error(`no matching ${label} event`)), timeoutMs)
        const check = () => {
            const found = buffer.find(match)
            if (found) {
                clearTimeout(timer)
                resolve(found)
            } else {
                setTimeout(check, 10)
            }
        }
        setTimeout(check, 10)
    })
}

describe('license events', () => {
    it('publishes created and countered events to both company channels', async (t) => {
        if (!redisAlive || !studioSub || !labelSub) return t.skip('redis not reachable')

        studioEvents = []
        labelEvents = []
        const { movieId, songId } = await seedFixture()

        const studioPromise = collectFrom(studioEvents, (e) => e.type === 'created' && e.song_id === songId, 'studio')
        const labelPromise = collectFrom(labelEvents, (e) => e.type === 'created' && e.song_id === songId, 'label')

        const lic = await request(app).post('/api/v1/licenses')
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ movie_id: movieId, scene_number: 1, song_id: songId, start_offset: 5, length: 20, license_fee: 1234 })
        assert.equal(lic.status, 200)

        for (const p of [studioPromise, labelPromise]) {
            const event = await p
            assert.equal(event.license_id, lic.body.id)
            assert.equal(event.to_state, 'OFFER')
            assert.equal(event.license_fee, 1234)
        }

        labelEvents = []
        const counterPromise = collectFrom(labelEvents, (e) => e.type === 'countered', 'counter')
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
