import { User } from 'knex/types/tables'

export const validateJWT = (token: string) => {
    return true
}

export const signJWT = (user: User) => {
    return ""
}
