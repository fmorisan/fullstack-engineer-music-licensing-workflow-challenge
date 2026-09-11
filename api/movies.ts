import { Router } from "express";
import db from "../db";
import z from "zod";
import { validateRequestBody } from "../middlewares";
import { uuidv7 } from "uuidv7";

const router = Router()

const STUDIO_ID = '9795f84a-6836-4e13-84e1-7a1db8bd24d0'

router.get('/', async (req, res) => {
    const movies = await db('movies').where('studio_id', STUDIO_ID)

    if (movies.length === 0) {
        return res.status(404).json({error: 'movie not found'})
    }

    res.json(movies)
})

const NewMovieSchema = z.object({
    name: z.string(),
    description: z.string()
})

router.post('/', validateRequestBody(NewMovieSchema), async (req, res) => {
    const inserted = await db('movies').insert({
        id: uuidv7() as any,
        name: req.body.name,
        description: req.body.description,
        studio_id: STUDIO_ID
    }).returning('*')

    if (inserted.length > 0) {
        res.json(inserted[0]!)
    } else {
        res.status(500).json({error: 'movie creation failed'})
    }
})

const NewSceneSchema = z.object({
    name: z.string(),
    start_offset: z.number(),
    length: z.number()
})

router.post('/:id/scenes', validateRequestBody(NewSceneSchema), async (req, res) => {
    const movie = await db('movies').where('id', req.params.id).first()
    if (!movie) {
        return res.status(404).json({error: 'movie not found'})
    }

    const scene = await db.transaction(async (t) => {
        const { max_scene_number } = await t('movie_scenes')
            .where('movie_id', req.params.id)
            .max({ max_scene_number: 'scene_number' })
            .first() ?? {}

        return await t('movie_scenes').insert({
            id: uuidv7() as any,
            movie_id: req.params.id,
            name: req.body.name,
            scene_number: (max_scene_number ?? 0) + 1,
            start_offset: req.body.start_offset,
            length: req.body.length
        }).returning('*')
    })

    res.json(scene[0]!)
})

router.get('/:id/scenes/:n', async (req, res) => {
    const scene = await db('movie_scenes')
        .where('movie_id', req.params.id)
        .andWhere('scene_number', req.params.n)
        .first()

    if (!scene) {
        return res.status(404).json({error: 'scene not found'})
    }

    res.json(scene)
})

const movies = router

export default movies
