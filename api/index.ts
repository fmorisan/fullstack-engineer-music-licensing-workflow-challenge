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
app.use((req, res, next) => { console.log(req.path); return next() })
app.use('/api/v1/auth', auth)
app.use('/api/v1/movies', movies)
app.use('/api/v1/songs', songs)
app.use('/api/v1/licenses', licenses)
app.get('/api/v1/health', (req, res) => res.json({ok: true}))

export default app
