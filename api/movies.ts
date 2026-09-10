import { Router } from "express";
import db from "../db";
import z from "zod";
import { validateRequestBody } from "../middlewares";
import { uuidv7 } from "uuidv7";

const router = Router()

router.get('/', async (req, res) => {
    const studio_id = '9795f84a-6836-4e13-84e1-7a1db8bd24d0'
    const movie = await db('movies').where('studio_id', studio_id)

    if (movie) {
        res.json(movie)
    } else {
        res.status(404).json({error: 'movie not found'})
    }
})

const NewMovieSchema = z.object({
    name: z.string(),
    description: z.string()
})

router.post('/', validateRequestBody(NewMovieSchema), async (req, res) => {
    const studio_id = '9795f84a-6836-4e13-84e1-7a1db8bd24d0'
    const movie = await db('movies').insert({
        id: uuidv7() as any,
        studio_id: studio_id
    }).returning('*')

    if (movie) {
        res.json(movie)
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
    await db.transaction(async (t) => {
        const scene_count = await t('movie_scenes')
            .where('movie_id', req.params.id)
            .max('scene_number', 'max_scene_number')
            .select('max_scene_number').first()

        await t('movie_scenes').insert({
            id: uuidv7() as any,
            movie_id: req.params.id,
            name: req.body.name,
            scene_number: scene_count + 1,
            start_offset: req.body.start_offset,
            length: req.body.length
        }).returning('id')
    })

    res.json(
        await db('movie_scenes').where('movie_id', req.params.id).orderBy('scene_number', 'desc').first()
    )
})

router.get('/:id/scenes/:n', async (req, res) => {
    const scene = await db('movie_scenes').where('movie_id', req.params.id).andWhere('scene_number', req.params.n)

    if (!scene) {
        res.status(404).json({error: 'scene not found'})
    }
    res.json(scene)
})

const movies = router

export default movies
