import { before, after, describe, it } from 'node:test'
import assert from 'node:assert'
import request from 'supertest'
import { getApp, setupDb, teardownDb, loginAs } from './helpers'

const STUDIO_EMAIL = 'grace@gotham.studios'
const LABEL_EMAIL = 'mark@evilrecords.com'

let app: any
let db: any

before(async () => {
    db = await setupDb()
    app = await getApp()
})

after(async () => {
    await teardownDb()
})

describe('auth', () => {
    it('rejects bad credentials', async () => {
        const res = await request(app).post('/api/v1/auth/login')
            .send({ email: STUDIO_EMAIL, password: 'wrong' })
        assert.equal(res.status, 401)
    })

    it('issues tokens on valid login', async () => {
        const res = await request(app).post('/api/v1/auth/login')
            .send({ email: STUDIO_EMAIL, password: 'password' })
        assert.equal(res.status, 200)
        assert.ok(res.body.access_token)
        assert.ok(res.body.refresh_token)
    })

    it('rejects me without token', async () => {
        const res = await request(app).get('/api/v1/auth/me')
        assert.equal(res.status, 401)
    })

    it('returns current user for valid token', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const res = await request(app).get('/api/v1/auth/me')
            .set('Authorization', `Bearer ${token}`)
        assert.equal(res.status, 200)
        assert.equal(res.body.email, STUDIO_EMAIL)
        assert.equal(res.body.user_type, 'movie_studio')
    })

    it('refreshes access tokens', async () => {
        const login = await request(app).post('/api/v1/auth/login')
            .send({ email: LABEL_EMAIL, password: 'password' })
        const res = await request(app).post('/api/v1/auth/refresh')
            .send({ refresh_token: login.body.refresh_token })
        assert.equal(res.status, 200)
        assert.ok(res.body.access_token)
    })

    it('invalidates refresh tokens on logout', async () => {
        const login = await request(app).post('/api/v1/auth/login')
            .send({ email: LABEL_EMAIL, password: 'password' })
        const refresh = login.body.refresh_token

        const out = await request(app).post('/api/v1/auth/logout')
            .send({ refresh_token: refresh })
        assert.equal(out.status, 204)

        const res = await request(app).post('/api/v1/auth/refresh')
            .send({ refresh_token: refresh })
        assert.equal(res.status, 401)
    })
})

describe('movies and scenes', () => {
    it('rejects unauthenticated access', async () => {
        const res = await request(app).get('/api/v1/movies')
        assert.equal(res.status, 401)
    })

    it('returns 404 when studio has no movies', async () => {
        const token = await loginAs(app, LABEL_EMAIL)
        const res = await request(app).get('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
        assert.equal(res.status, 404)
    })

    it('forbids labels from creating movies', async () => {
        const token = await loginAs(app, LABEL_EMAIL)
        const res = await request(app).post('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'X', description: 'Y' })
        assert.equal(res.status, 403)
    })

    it('creates movies for the studio company', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const res = await request(app).post('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'Test Reel', description: 'Integration test' })
        assert.equal(res.status, 200)
        assert.equal(res.body.name, 'Test Reel')
        assert.equal(res.body.studio_id, '849a4821-fb9a-4c6a-8fff-662cc37f6802')
    })

    it('numbers scenes sequentially', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const movies = await request(app).get('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
        const movieId = movies.body[0].id

        const first = await request(app).post(`/api/v1/movies/${movieId}/scenes`)
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'Opening', start_offset: 0, length: 120 })
        assert.equal(first.status, 200)
        assert.equal(first.body.scene_number, 1)

        const second = await request(app).post(`/api/v1/movies/${movieId}/scenes`)
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'Climax', start_offset: 120, length: 90 })
        assert.equal(second.status, 200)
        assert.equal(second.body.scene_number, 2)
    })

    it('returns 404 for unknown scene numbers', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const movies = await request(app).get('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
        const movieId = movies.body[0].id

        const res = await request(app).get(`/api/v1/movies/${movieId}/scenes/99`)
            .set('Authorization', `Bearer ${token}`)
        assert.equal(res.status, 404)
    })

    it('lists scenes for a movie', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const movies = await request(app).get('/api/v1/movies')
            .set('Authorization', `Bearer ${token}`)
        const movieId = movies.body[0].id

        const res = await request(app).get(`/api/v1/movies/${movieId}/scenes`)
            .set('Authorization', `Bearer ${token}`)
        assert.equal(res.status, 200)
        assert.ok(Array.isArray(res.body))
        assert.ok(res.body.length >= 2)
        assert.ok(res.body[0].scene_number <= res.body[1].scene_number)
    })
})

describe('songs', () => {
    it('forbids studios from creating songs', async () => {
        const token = await loginAs(app, STUDIO_EMAIL)
        const res = await request(app).post('/api/v1/songs')
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'No', author: 'X', length_seconds: 100 })
        assert.equal(res.status, 403)
    })

    it('creates songs for the label company', async () => {
        const token = await loginAs(app, LABEL_EMAIL)
        const res = await request(app).post('/api/v1/songs')
            .set('Authorization', `Bearer ${token}`)
            .send({ name: 'Neon Skyline', author: 'The Volts', length_seconds: 214 })
        assert.equal(res.status, 200)
        assert.equal(res.body.name, 'Neon Skyline')
        assert.equal(res.body.length_seconds, 214)
    })
})

describe('licensing workflow', () => {
    let studioToken: string
    let labelToken: string
    let movieId: string
    let songId: string
    let licenseId: string

    before(async () => {
        studioToken = await loginAs(app, STUDIO_EMAIL)
        labelToken = await loginAs(app, LABEL_EMAIL)

        const movie = await request(app).post('/api/v1/movies')
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ name: 'Negotiation', description: 'A negotiation story' })
        movieId = movie.body.id

        await request(app).post(`/api/v1/movies/${movieId}/scenes`)
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ name: 'Scene 1', start_offset: 0, length: 60 })

        const song = await request(app).post('/api/v1/songs')
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ name: 'Deal Music', author: 'The Brokers', length_seconds: 180 })
        songId = song.body.id
    })

    it('forbids labels from creating licenses', async () => {
        const res = await request(app).post('/api/v1/licenses')
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ movie_id: movieId, scene_number: 1, song_id: songId, start_offset: 10, length: 30, license_fee: 5000 })
        assert.equal(res.status, 403)
    })

    it('creates an opening offer', async () => {
        const res = await request(app).post('/api/v1/licenses')
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ movie_id: movieId, scene_number: 1, song_id: songId, start_offset: 10, length: 30, license_fee: 5000 })
        assert.equal(res.status, 200)
        assert.equal(res.body.state, 'OFFER')
        assert.equal(res.body.license_fee, 5000)
        licenseId = res.body.id
    })

    it('requires a fee for counters', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ state: 'COUNTER' })
        assert.equal(res.status, 400)
    })

    it('forbids studios from countering their own offer', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ state: 'COUNTER', license_fee: 100 })
        assert.equal(res.status, 403)
    })

    it('lets labels counter an offer', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ state: 'COUNTER', license_fee: 4200 })
        assert.equal(res.status, 200)
        assert.equal(res.body.state, 'COUNTER')
        assert.equal(res.body.license_fee, 4200)
    })

    it('forbids countering a counter', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ state: 'COUNTER', license_fee: 1 })
        assert.equal(res.status, 403)
    })

    it('lets studios re-offer after a counter', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ state: 'OFFER', license_fee: 4800 })
        assert.equal(res.status, 200)
        assert.equal(res.body.state, 'OFFER')
    })

    it('lets labels accept an offer', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${labelToken}`)
            .send({ state: 'ACCEPTED' })
        assert.equal(res.status, 200)
        assert.equal(res.body.state, 'ACCEPTED')
    })

    it('rejects updates after the license is closed', async () => {
        const res = await request(app).patch(`/api/v1/licenses/${licenseId}`)
            .set('Authorization', `Bearer ${studioToken}`)
            .send({ state: 'REJECTED' })
        assert.equal(res.status, 403)
    })

    it('lists movie licenses with song and state', async () => {
        const res = await request(app).get(`/api/v1/movies/${movieId}/licenses`)
            .set('Authorization', `Bearer ${studioToken}`)
        assert.equal(res.status, 200)
        const licenses = res.body.licenses
        assert.ok(Array.isArray(licenses))
        assert.ok(licenses.some((l: any) => l.id === licenseId && l.state === 'ACCEPTED'))
    })

    it('lists scene licenses with song details', async () => {
        const res = await request(app).get(`/api/v1/movies/${movieId}/scenes/1/licenses`)
            .set('Authorization', `Bearer ${studioToken}`)
        assert.equal(res.status, 200)
        assert.ok(Array.isArray(res.body))
        assert.ok(res.body.some((l: any) => l.id === licenseId && l.state === 'ACCEPTED' && l.song_name === 'Deal Music'))
    })

    it('lists song licenses for the label', async () => {
        const res = await request(app).get(`/api/v1/songs/${songId}/licenses`)
            .set('Authorization', `Bearer ${labelToken}`)
        assert.equal(res.status, 200)
        assert.ok(Array.isArray(res.body))
        assert.ok(res.body.some((l: any) => l.id === licenseId))
    })
})
