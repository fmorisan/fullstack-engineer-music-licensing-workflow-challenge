import jwt, { JwtPayload } from 'jsonwebtoken'
import { scryptSync, randomBytes, timingSafeEqual } from 'crypto'
import { CompanyType, User } from 'knex/types/tables'

export interface AuthClaims {
    user_id: string,
    company_id: string,
    user_type: CompanyType
}

const JWT_SECRET = process.env.JWT_SECRET ?? 'dev-only-secret-do-not-use-in-prod'

export const signJWT = (user: User, user_type: CompanyType) => {
    const claims: AuthClaims = {
        user_id: user.id,
        company_id: user.employer,
        user_type
    }
    return jwt.sign(claims, JWT_SECRET, { expiresIn: '1h' })
}

export const validateJWT = (authorization?: string): (JwtPayload & AuthClaims) | null => {
    if (!authorization) return null
    const token = authorization.startsWith('Bearer ') ? authorization.slice(7) : authorization
    try {
        const payload = jwt.verify(token, JWT_SECRET)
        if (typeof payload === 'string') return null
        return payload as JwtPayload & AuthClaims
    } catch {
        return null
    }
}

export const hashPassword = (password: string) => {
    const salt = randomBytes(16).toString('hex')
    const hash = scryptSync(password, salt, 64).toString('hex')
    return `${salt}:${hash}`
}

export const verifyPassword = (password: string, stored: string) => {
    const [salt, hash] = stored.split(':')
    if (!salt || !hash) return false
    const expected = Buffer.from(hash, 'hex')
    const actual = scryptSync(password, salt, expected.length)
    return timingSafeEqual(expected, actual)
}
