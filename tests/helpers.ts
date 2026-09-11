import { before, after } from 'node:test'
import fs from 'node:fs'
import knex, { Knex } from 'knex'
import request from 'supertest'
import config from '../knexfile'

process.env.KNEX_ENV = 'test'
process.env.JWT_SECRET = 'test-secret'

let _db: Knex | null = null

export async function setupDb() {
    await fs.promises.rm('./test.sqlite3', { force: true })
    const db = knex((config as any).test)
    await db.migrate.latest()
    await db.seed.run()
    _db = db
    return db
}

export async function getApp() {
    const mod = await import('../api')
    return mod.default
}

export async function teardownDb() {
    if (_db) {
        await _db.destroy()
        _db = null
    }
    const { default: appDb } = await import('../db')
    await appDb.destroy()
    await fs.promises.rm('./test.sqlite3', { force: true })
}

export async function loginAs(app: any, email: string): Promise<string> {
    const res = await request(app).post('/api/v1/auth/login').send({ email, password: 'password' })
    return res.body.access_token
}
