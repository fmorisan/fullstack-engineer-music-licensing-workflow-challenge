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

    const result = await LicenseController.updateLicense(req.auth!, req.params.id, req.body)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

export default licenses
