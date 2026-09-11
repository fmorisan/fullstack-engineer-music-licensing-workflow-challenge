import z from "zod";
import { AuthClaims } from "./login";
import db from "../db";
import { uuidv7 } from "uuidv7";

const CreateSongSchema = z.object({
    name: z.string(),
    author: z.string(),
    length_seconds: z.number().int().positive(),
})

type CreateSongData = z.infer<typeof CreateSongSchema>

const createSong = async (user: AuthClaims, songData: CreateSongData) => {
    const song = await db('songs').insert({
        id: uuidv7() as any,
        name: songData.name,
        author: songData.author,
        length_seconds: songData.length_seconds,
        label_id: user.company_id as any
    }).returning('*').first()

    if (!song) {
        return {
            success: false,
            error: 'could not create song'
        }
    }

    // TODO: trigger indexing?

    return {
        success: true,
        value: song
    }
}

const uploadSongBoxArt = async (user: AuthClaims) => {
    // TODO call S3 and get pre-signed URL
}

const SongController = {
    CreateSongSchema,
    createSong,
    uploadSongBoxArt
}

export default SongController

