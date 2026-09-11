import { Router } from "express";
import SongController from "../controllers/song";
import { isUserType, validateRequestBody } from "../middlewares";

const songs = Router()

songs.post('/',
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

export default songs
