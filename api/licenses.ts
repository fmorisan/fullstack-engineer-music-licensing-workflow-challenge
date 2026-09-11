import { Router } from "express";
import LicenseController from "../controllers/licenses";
import { validateRequestBody, isLoggedIn, isUserType } from "../middlewares";

const licenses = Router()

licenses.post('/',
    isLoggedIn,
    isUserType('movie_studio'),
    validateRequestBody(LicenseController.NewLicenseSchema),
    async (req, res) => {

    const result = await LicenseController.createLicense(req.auth!, req.body)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

licenses.patch('/:id',
    isLoggedIn,
    validateRequestBody(LicenseController.UpdateLicenseSchema),
    async (req, res) => {

    const result = await LicenseController.updateLicense(req.auth!, req.params.id as string, req.body)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

licenses.get('/events',
    isLoggedIn,
    (req, res) => {
        res.setHeader('Connection', 'keep-alive')
        res.setHeader('Cache-Control', 'no-cache')
        res.setHeader('Content-Type', 'text/event-stream')
        res.flushHeaders()

        // TODO: connect to pubsub, get events

        let id = 1
        const interval = setInterval(() => {
            res.write(JSON.stringify({event: 'heartbeat', id: id++}))
        }, 500)

        res.on('close', () => {
            clearInterval(interval)
            res.end()
        })
})

export default licenses
