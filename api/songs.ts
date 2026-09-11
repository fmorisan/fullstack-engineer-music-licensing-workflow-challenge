import { Router } from "express";
import SongController from "../controllers/song";
import { isLoggedIn, isUserType, validateRequestBody } from "../middlewares";
import z from "zod";

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

const SearchQuerySchema = z.object({
    q: z.string().trim().optional(),
    limit: z.coerce.number().int().min(1).max(100).default(25),
    offset: z.coerce.number().int().min(0).default(0)
})

songs.get('/search', isLoggedIn, async (req, res) => {
    const parsed = SearchQuerySchema.safeParse(req.query)

    if (!parsed.success) {
        return res.status(400).json({error: 'malformed query'})
    }

    res.json(await SongController.searchSongs(parsed.data))
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
