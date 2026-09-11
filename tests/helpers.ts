import fs from 'node:fs'
import knex, { Knex } from 'knex'
import request from 'supertest'
import config from '../knexfile'
import app from '../api'
import appDb from '../db'
import { closeRedis } from '../redis'

let _db: Knex | null = null

export async function setupDb() {
    if (process.env.NODE_ENV !== 'test') {
        throw new Error('tests must run with NODE_ENV=test (use pnpm test)')
    }

    await fs.promises.rm('./test.sqlite3', { force: true })
    const db = knex((config as any).test)
    await db.migrate.latest()
    await db.seed.run()
    _db = db
    return db
}

export function getApp() {
    return app
}

export async function teardownDb() {
    if (_db) {
        await _db.destroy()
        _db = null
    }
    await appDb.destroy()
    await closeRedis()
    await fs.promises.rm('./test.sqlite3', { force: true })
}

export async function loginAs(app: any, email: string): Promise<string> {
    const res = await request(app).post('/api/v1/auth/login').send({ email, password: 'password' })
    return res.body.access_token
}
