import z from "zod"
import { AuthClaims } from "./login"
import { LicenseState, SongLicense } from "knex/types/tables"
import db from "../db"
import { getSubscriber, probeRedis, publishLicenseEvent } from "../redis"
import { uuidv7 } from "uuidv7"
import { Response } from "express"

type Result<T, E> = {success: true, value: T} | {success: false, error: E, status?: number}

const NewLicenseSchema = z.strictObject({
    movie_id: z.uuid(),
    scene_number: z.number(),
    song_id: z.uuid(),
    start_offset: z.number(),
    length: z.number(),
    license_fee: z.number()
})

type NewLicenseData = z.output<typeof NewLicenseSchema>

const createLicense = async (user: AuthClaims, licenseData: NewLicenseData): Promise<Result<SongLicense, string>> => {
    if (user.user_type !== 'movie_studio') {
        return {
            success: false,
            error: 'not a movie studio user',
            status: 403
        }
    }

    const movie = await db('movies').where('id', licenseData.movie_id).andWhere('studio_id', user.company_id).first()

    if (!movie) {
        return {
            success: false,
            error: `movie ${licenseData.movie_id} does not exist under studio ${user.company_id}`,
            status: 404
        }
    }

    const scene = await db('movie_scenes').where('movie_id', movie.id).andWhere('scene_number', licenseData.scene_number).first()

    if (!scene) {
        return {
            success: false,
            error: `scene #${licenseData.scene_number} does not exist under movie ${licenseData.movie_id}`,
            status: 404
        }
    }

    const song = await db('songs').where('id', licenseData.song_id).first()

    if (!song) {
        return {
            success: false,
            error: `song ${licenseData.song_id} does not exist`,
            status: 400
        }
    }

    const license = await db('licenses').insert({
        id: uuidv7() as any,
        scene_id: scene.id,
        song_id: licenseData.song_id as SongLicense['song_id'],
        start_offset: licenseData.start_offset,
        length: licenseData.length,
        license_fee: licenseData.license_fee,
        state: 'OFFER'
    }).returning('*')

    const created = license[0]!

    publishLicenseEvent(
        [`events:${user.company_id}`, `events:${song.label_id}`],
        {
            type: 'created',
            license_id: created.id,
            song_id: created.song_id,
            from_state: null,
            to_state: 'OFFER',
            license_fee: created.license_fee,
            at: new Date().toISOString()
        }
    ).catch(err => console.error('license event publish failed:', err.message))

    return {success: true, value: created}
}

const UpdateLicenseSchema = z.discriminatedUnion('state', [
    z.object({
        state: z.enum(['OFFER', 'COUNTER']),
        license_fee: z.number()
    }),
    z.object({
        state: z.enum(['ACCEPTED', 'REJECTED']),
    })
])

type UpdateLicenseData = z.infer<typeof UpdateLicenseSchema>

function canOffer(user: AuthClaims, license: SongLicense) {
    if (license.state !== 'COUNTER') {
        return false
    }
    if (user.user_type !== 'movie_studio') {
        return false
    }

    return true
}

function canCounter(user: AuthClaims, license: SongLicense) {
    if (license.state !== 'OFFER') {
        return false
    }
    if (user.user_type !== 'record_label') {
        return false
    }

    return true
}

function canClose(user: AuthClaims, license: SongLicense) {
    if (license.state === 'OFFER' && user.user_type === 'record_label') {
        return true
    }
    if (license.state === 'COUNTER' && user.user_type === 'movie_studio') {
        return true
    }

    return false
}

async function applyTransition(licenseId: string, fromState: LicenseState, updates: Partial<SongLicense>): Promise<Result<SongLicense, string>> {
    const updated = await db('licenses')
        .update(updates)
        .where('id', licenseId)
        .andWhere('state', fromState)
        .returning('*')

    let update = updated.at(0)

    if (!update) {
        return {
            success: false,
            error: `license is no longer in state ${fromState}`,
            status: 409
        }
    }

    // TODO: notify

    return {
        success: true,
        value: update
    }
}

const updateLicense = async (user: AuthClaims, licenseId: string, updateData: UpdateLicenseData): Promise<Result<SongLicense, string>> => {
    const license = await db('licenses').where('licenses.id', licenseId)
        .join('movie_scenes', 'licenses.scene_id', 'movie_scenes.id')
        .join('movies', 'movie_scenes.movie_id', 'movies.id')
        .join('songs', 'licenses.song_id', 'songs.id')
        .select('licenses.*', 'movies.studio_id', 'songs.label_id')
        .first()

    if (!license) {
        return {
            success: false,
            error: `license ${licenseId} does not exist`,
            status: 404
        }
    }

    // NOTE: we don't leak existence of licenses to non-parties
    if (user.company_id !== license.studio_id && user.company_id !== license.label_id) {
        return {
            success: false,
            error: `license ${licenseId} does not exist`,
            status: 404
        }
    }
    let result: Result<SongLicense, string>
    switch (updateData.state) {
        case "OFFER":
            if (!canOffer(user, license)) {
                return { success: false, error: 'only movie studios can re-offer after a counter', status: 403 }
            }
            result = await applyTransition(licenseId, 'COUNTER', { license_fee: updateData.license_fee, state: 'OFFER' })
            break
        case "COUNTER":
            if (!canCounter(user, license)) {
                return { success: false, error: 'only record labels can counter an offer', status: 403 }
            }
            result = await applyTransition(licenseId, 'OFFER', { license_fee: updateData.license_fee, state: 'COUNTER' })
            break
        case "ACCEPTED":
            if (!canClose(user, license)) {
                return { success: false, error: 'you cannot accept this license in its current state', status: 403 }
            }
            result = await applyTransition(licenseId, license.state, { state: 'ACCEPTED' })
            break
        case "REJECTED":
            if (!canClose(user, license)) {
                return { success: false, error: 'you cannot reject this license in its current state', status: 403 }
            }
            result = await applyTransition(licenseId, license.state, { state: 'REJECTED' })
            break
        default:
            return {
                success: false,
                error: 'unreachable'
            }
    }

    if (result.success) {
        publishLicenseEvent(
            [`events:${license.studio_id}`, `events:${license.label_id}`],
            {
                type: ({ OFFER: 'offered', COUNTER: 'countered', ACCEPTED: 'accepted', REJECTED: 'rejected' } as const)[updateData.state],
                license_id: licenseId,
                song_id: license.song_id,
                from_state: license.state,
                to_state: result.value.state,
                license_fee: result.value.license_fee,
                at: new Date().toISOString()
            }
        ).catch(err => console.error('license event publish failed:', err.message))
    }

    return result
}

const streamClients = new Map<string, Set<Response>>()

const ensureSubscription = async (channel: string, res: Response) => {
    const sub = await getSubscriber()
    if (!sub) {
        throw new Error('redis unavailable')
    }

    let clients = streamClients.get(channel)
    if (!clients) {
        clients = new Set()
        streamClients.set(channel, clients)
        await sub.subscribe(channel, (message) => {
            for (const client of streamClients.get(channel) ?? []) {
                client.write(`event: license\ndata: ${message}\n\n`)
            }
        })
    }
    clients.add(res)
}

const releaseSubscription = async (channel: string, res: Response) => {
    const clients = streamClients.get(channel)
    if (!clients) {
        return
    }

    clients.delete(res)
    if (clients.size === 0) {
        streamClients.delete(channel)
        const sub = await getSubscriber()
        if (sub) {
            await sub.unsubscribe(channel).catch(() => {})
        }
    }
}

const licenseEventStream = async (user: AuthClaims, res: Response) => {
    const alive = await probeRedis(1000)
    if (!alive) {
        return res.status(503).json({error: 'realtime unavailable'})
    }

    const channel = `events:${user.company_id}`

    try {
        await ensureSubscription(channel, res)
    } catch (err) {
        return res.status(503).json({error: 'realtime unavailable'})
    }

    res.setHeader('Connection', 'keep-alive')
    res.setHeader('Cache-Control', 'no-cache')
    res.setHeader('Content-Type', 'text/event-stream')
    res.setHeader('X-Accel-Buffering', 'no')
    res.flushHeaders()

    res.write('retry: 3000\n\n')
    res.write(`event: connected\ndata: {"channel":"${channel}"}\n\n`)

    const heartbeat = setInterval(() => {
        res.write(': ping\n\n')
    }, 25000)

    res.on('close', () => {
        clearInterval(heartbeat)
        releaseSubscription(channel, res)
    })
}

const LicenseController = {
    NewLicenseSchema,
    createLicense,
    UpdateLicenseSchema,
    updateLicense,
    licenseEventStream
}

export default LicenseController
