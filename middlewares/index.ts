import { Response, Request, RequestHandler, NextFunction } from 'express';
import { CompanyType } from 'knex/types/tables';
import { z, ZodObject, ZodError, ZodType } from 'zod';
import { AuthClaims, validateJWT } from '../controllers/login';

declare global {
    namespace Express {
        interface Request {
            auth?: AuthClaims
        }
    }
}

type ErrorListItem = { errors: ZodError<any> };

export const sendErrors: (errors: Array<ErrorListItem>, res: Response) => void = (errors, res) => {
  return res.status(400).send({ errors })
};

export const validateRequestBody = <T extends ZodType>(schema: T): RequestHandler<any, any, z.output<T>> =>
    (req: Request, res: Response, next: NextFunction) => {
        const parsed = schema.safeParse(req.body);
        if (parsed.success) {
            return next();
        } else {
            return sendErrors([{ errors: parsed.error }], res);
        }
};

export const isLoggedIn: RequestHandler = (req, res, next) => {
    const claims = validateJWT(req.headers.authorization)

    if (!claims) {
        return res.status(401).json({error: 'unauthorized'})
    }

    req.auth = claims
    return next()
}

export const isUserType = (type: CompanyType): RequestHandler => (req, res, next) => {
    if (req.auth?.user_type !== type) {
        return res.status(403).json({error: 'forbidden'})
    }

    return next()
}
