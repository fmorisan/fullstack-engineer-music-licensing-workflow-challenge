import { Router } from "express";
import { validateRequestBody } from "../middlewares";
import { findRefreshToken, issueRefreshToken, revokeRefreshToken, signJWT, verifyPassword } from "../controllers/login";
import db from "../db";
import z from "zod"

const auth  = Router()
const UserLoginSchema = z.object({
    email: z.string(),
    password: z.string()
})

auth.post('/login', validateRequestBody(UserLoginSchema), async (req, res) => {
    const user = await db('users')
        .join('companies', 'companies.id', 'users.employer')
        .where('users.email', req.body.email)
        .select('users.*', 'companies.kind')
        .first()

    if (!user || !verifyPassword(req.body.password, user.password_hash)) {
        return res.status(401).json({error: 'invalid credentials'})
    }

    const access_token = signJWT(user, user.kind)
    const refresh_token = await issueRefreshToken(user.id)

    res.json({access_token, refresh_token})
})

const RefreshSchema = z.object({
    refresh_token: z.string()
})

auth.post('/refresh', validateRequestBody(RefreshSchema), async (req, res) => {
    const stored = await findRefreshToken(req.body.refresh_token)
    if (!stored) {
        return res.status(401).json({error: 'invalid refresh token'})
    }

    const user = await db('users')
        .join('companies', 'companies.id', 'users.employer')
        .where('users.id', stored.user_id)
        .select('users.*', 'companies.kind')
        .first()

    if (!user) {
        return res.status(401).json({error: 'invalid refresh token'})
    }

    res.json({
        access_token: signJWT(user, user.kind),
        refresh_token: req.body.refresh_token
    })
})

auth.post('/logout', validateRequestBody(RefreshSchema), async (req, res) => {
    await revokeRefreshToken(req.body.refresh_token)
    res.status(204).end()
})

auth.get('/me', (req, res) => {
    res.send('ok').end()
})

export default auth
