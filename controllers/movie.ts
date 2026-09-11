import z from "zod"
import { AuthClaims } from "./login"
import { Movie, MovieScene } from "knex/types/tables"
import db from "../db"
import { uuidv7 } from "uuidv7"

type Result<T, E> = {success: true, value: T} | {success: false, error: E, status?: number}

const NewMovieSchema = z.object({
    name: z.string(),
    description: z.string()
})

type NewMovieData = z.output<typeof NewMovieSchema>

const NewSceneSchema = z.object({
    name: z.string(),
    start_offset: z.number(),
    length: z.number()
})

type NewSceneData = z.output<typeof NewSceneSchema>

const listMovies = async (user: AuthClaims): Promise<Result<Movie[], string>> => {
    const movies = await db('movies').where('studio_id', user.company_id)

    if (movies.length === 0) {
        return {
            success: false,
            error: 'movie not found',
            status: 404
        }
    }

    return {success: true, value: movies}
}

const createMovie = async (user: AuthClaims, movieData: NewMovieData): Promise<Result<Movie, string>> => {
    const inserted = await db('movies').insert({
        id: uuidv7() as any,
        name: movieData.name,
        description: movieData.description,
        studio_id: user.company_id as any
    }).returning('*')

    if (inserted.length === 0) {
        return {
            success: false,
            error: 'movie creation failed',
            status: 500
        }
    }

    return {success: true, value: inserted[0]!}
}

const createScene = async (user: AuthClaims, movieId: string, sceneData: NewSceneData): Promise<Result<MovieScene, string>> => {
    const movie = await db('movies').where('id', movieId).andWhere('studio_id', user.company_id).first()

    if (!movie) {
        return {
            success: false,
            error: `movie ${movieId} does not exist under studio ${user.company_id}`,
            status: 404
        }
    }

    const scene = await db.transaction(async (t) => {
        const { max_scene_number } = await t('movie_scenes')
            .where('movie_id', movieId)
            .max({ max_scene_number: 'scene_number' })
            .first() ?? {}

        return await t('movie_scenes').insert({
            id: uuidv7() as any,
            movie_id: movieId,
            name: sceneData.name,
            scene_number: (max_scene_number ?? 0) + 1,
            start_offset: sceneData.start_offset,
            length: sceneData.length
        }).returning('*')
    })

    return {success: true, value: scene[0]!}
}

const getScene = async (user: AuthClaims, movieId: string, sceneNumber: number): Promise<Result<MovieScene, string>> => {
    const scene = await db('movie_scenes')
        .join('movies', 'movie_scenes.movie_id', 'movies.id')
        .where('movie_scenes.movie_id', movieId)
        .andWhere('movies.studio_id', user.company_id)
        .andWhere('movie_scenes.scene_number', sceneNumber)
        .select('movie_scenes.*')
        .first()

    if (!scene) {
        return {
            success: false,
            error: 'scene not found',
            status: 404
        }
    }

    return {success: true, value: scene}
}

const MovieController = {
    NewMovieSchema,
    NewSceneSchema,
    listMovies,
    createMovie,
    createScene,
    getScene
}

export default MovieController
