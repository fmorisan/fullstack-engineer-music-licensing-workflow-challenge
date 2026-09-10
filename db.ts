import { UUID } from "crypto"
import knex from "knex"
import config from "./knexfile"

declare module 'knex/types/tables' {
    type CompanyType = 'movie_studio' | 'record_label'

    interface User {
        id: UUID,
        username: string,
        email: string,
        password_hash: string,
        employer: UUID
    }

    interface Company { id: UUID,
        name: string,
        kind: CompanyType
    }

    interface Movie {
        id: UUID,
        name: string,
        description: string,
        // object-storage key for movie poster
        poster?: string
        studio_id: UUID
    }

    interface Song {
        id: UUID,
        name: string,
        author: string,
        // object-storage key for song boxart
        boxart?: string,
        label_id: UUID
    }

    interface MovieScene {
        id: UUID,
        movie_id: UUID,

        name: string,
        scene_number: number,
        start_offset: number,
        length: number
    }

    type LicenseState = 'OFFER' | 'COUNTER' | 'REJECTED' | 'ACCEPTED'

    interface SongLicense {
        id: UUID,
        scene_id: UUID,
        song_id: UUID,
        start_offset: number,
        length: number,
        license_fee: number,
        state: LicenseState
    }

    interface Tables {
        users: User,
        companies: Company,
    
        movies: Movie,
        songs: Song,

        movie_scenes: MovieScene,
        licenses: SongLicense
    }
}

const db = knex(config.development)

export default db
