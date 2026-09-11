import z from "zod"
import { AuthClaims } from "./login"
import { LicenseState, SongLicense } from "knex/types/tables"
import db from "../db"
import { uuidv7 } from "uuidv7"

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

    return {success: true, value: license[0]!}
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

    if (updated.length === 0) {
        return {
            success: false,
            error: `license is no longer in state ${fromState}`,
            status: 409
        }
    }

    // TODO: notify

    return {
        success: true,
        value: updated[0]!
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

    switch (updateData.state) {
        case "OFFER":
            if (!canOffer(user, license)) {
                return { success: false, error: 'only movie studios can re-offer after a counter', status: 403 }
            }
            return await applyTransition(licenseId, 'COUNTER', { license_fee: updateData.license_fee, state: 'OFFER' })
        case "COUNTER":
            if (!canCounter(user, license)) {
                return { success: false, error: 'only record labels can counter an offer', status: 403 }
            }
            return await applyTransition(licenseId, 'OFFER', { license_fee: updateData.license_fee, state: 'COUNTER' })
        case "ACCEPTED":
            if (!canClose(user, license)) {
                return { success: false, error: 'you cannot accept this license in its current state', status: 403 }
            }
            return await applyTransition(licenseId, license.state, { state: 'ACCEPTED' })
        case "REJECTED":
            if (!canClose(user, license)) {
                return { success: false, error: 'you cannot reject this license in its current state', status: 403 }
            }
            return await applyTransition(licenseId, license.state, { state: 'REJECTED' })
    }

    return {
        success: false,
        error: 'unreachable'
    }
}

const LicenseController = {
    NewLicenseSchema,
    createLicense,
    UpdateLicenseSchema,
    updateLicense
}

export default LicenseController
