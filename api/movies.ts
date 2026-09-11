import { Router } from "express";
import MovieController from "../controllers/movie";
import { validateRequestBody, isLoggedIn, isUserType } from "../middlewares";

const router = Router()

router.get('/', isLoggedIn, async (req, res) => {
    const result = await MovieController.listMovies(req.auth!)
    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

router.post('/',
    isLoggedIn,
    isUserType('movie_studio'),
    validateRequestBody(MovieController.NewMovieSchema),
    async (req, res) => {

    const result = await MovieController.createMovie(req.auth!, req.body)
    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

router.get('/:id',
        isLoggedIn,
        isUserType('movie_studio'),
        async (req, res) => {
    const result = await MovieController.getMovie(req.auth!, req.params.id as any)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(404).json({error: 'not found'})
    }
})

router.get('/:id/licenses',
        isLoggedIn,
        isUserType('movie_studio'),
        async (req, res) => {
    const result = await MovieController.getMovieLicenses(req.auth!, req.params.id as any)

    if (result.success) {
        res.json(result.value)
    } else {
        res.status(404).json({error: 'not found'})
    }
})

router.post('/:id/scenes',
    isLoggedIn,
    isUserType('movie_studio'),
    validateRequestBody(MovieController.NewSceneSchema),
    async (req, res) => {

    const result = await MovieController.createScene(req.auth!, req.params.id as any, req.body)
    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

router.get('/:id/scenes/:n', isLoggedIn, async (req, res) => {
    const result = await MovieController.getScene(req.auth!, req.params.id as any, Number(req.params.n))
    if (result.success) {
        res.json(result.value)
    } else {
        res.status(result.status ?? 400).json({error: result.error})
    }
})

const movies = router

export default movies
