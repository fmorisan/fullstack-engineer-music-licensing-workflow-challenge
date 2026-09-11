import express, { NextFunction, Request, Response } from 'express'
import z from 'zod'
import { validateRequestBody } from '../middlewares'
import db from '../db'
import auth from './auth'
import movies from './movies'
import songs from './songs'
import licenses from './licenses'

const app = express()

app.use(express.json())
app.use('/api/v1/auth', auth)
app.use('/api/v1/movies', movies)
app.use('/api/v1/songs', songs)
app.use('/api/v1/licenses', licenses)
app.get('/api/v1/health', (req, res) => res.json({ok: true}))

app.use((err: unknown, req: Request, res: Response, _next: NextFunction) => {
    console.error(err)
    const status = (err as any)?.status ?? (err as any)?.statusCode ?? 500
    res.status(status).json({error: status >= 500 ? 'internal server error' : err instanceof Error ? err.message : 'bad request'})
})

export default app
