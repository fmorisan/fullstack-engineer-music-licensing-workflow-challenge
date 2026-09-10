import { Router } from "express";
import { validateRequestBody } from "../middlewares";
import z from "zod"

const auth  = Router()
const UserLoginSchema = z.object({
    email: z.string(),
    password: z.string()
})

auth.post('/login', validateRequestBody(UserLoginSchema), (req, res) => {
    res.send('ok').end()
})

auth.get('/me', (req, res) => {
    res.send('ok').end()
})

export default auth
