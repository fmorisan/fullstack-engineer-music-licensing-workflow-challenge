import z from "zod";
import { AuthClaims } from "./login";
import db from "../db";
import { uuidv7 } from "uuidv7";
import { Song, SongLicense } from "knex/types/tables";

type Result<T, E> = {success: true, value: T} | {success: false, error: E, status?: number}

const CreateSongSchema = z.object({
    name: z.string(),
    author: z.string(),
    length_seconds: z.number().int().positive(),
})

type CreateSongData = z.infer<typeof CreateSongSchema>

const createSong = async (user: AuthClaims, songData: CreateSongData): Promise<Result<Song, string>> => {
    const inserted = await db('songs').insert({
        id: uuidv7() as any,
        name: songData.name, author: songData.author,
        length_seconds: songData.length_seconds,
        label_id: user.company_id as any
    }).returning('*')

    const song = inserted[0]

    if (!song) {
        return {
            success: false,
            error: 'could not create song',
            status: 500
        }
    }

    // TODO: trigger indexing?

    return {
        success: true,
        value: song
    }
}

const getSong = async (id: string) => {
    return await db('songs').where('id', id).first()
}

const getSongLicenses = async (user: AuthClaims, id: string): Promise<Result<SongLicense[], string>> => {
    const song = await db('songs').where('id', id).andWhere('label_id', user.company_id).first()

    if (!song) {
        return {
            success: false,
            error: 'song not found',
            status: 404
        }
    }

    const licenses = await db('licenses').where('song_id', song.id)

    return {
        success: true,
        value: licenses
    }
}

const uploadSongBoxArt = async (user: AuthClaims) => {
    // TODO call S3 and get pre-signed URL
}

const SongController = {
    CreateSongSchema,
    createSong,
    uploadSongBoxArt,
    getSong,
    getSongLicenses
}

export default SongController

