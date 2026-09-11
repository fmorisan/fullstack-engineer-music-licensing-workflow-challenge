import { Router } from "express";
import SongController from "../controllers/song";
import { isLoggedIn, isUserType, validateRequestBody } from "../middlewares";

const songs = Router()

songs.post('/',
        isLoggedIn,
        isUserType('record_label'),
        validateRequestBody(SongController.CreateSongSchema),
        async (req, res) => {

    const result = await SongController.createSong(req.auth!, req.body)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(400).json({error: result.error})
    }
})

songs.get('/:id',
        isLoggedIn,
        async (req, res) => {
    const result = await SongController.getSong(req.params.id as any)

    if (!result) {
        return res.status(404).json({error: 'not found'})
    }

    res.json(result)
})

songs.get('/:id/licenses',
        isLoggedIn,
        isUserType('record_label'),
        async (req, res) => {
    const result = await SongController.getSongLicenses(req.auth!, req.params.id as any)

    if (!result.success) {
        return res.status(result.status ?? 404).json({error: result.error})
    }

    res.json(result.value)
})

export default songs
