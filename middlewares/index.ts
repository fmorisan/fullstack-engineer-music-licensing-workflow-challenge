import { Response, Request, RequestHandler, NextFunction } from 'express';
import { CompanyType, User } from 'knex/types/tables';
import { z, ZodObject, ZodError, ZodType } from 'zod';
import { validateJWT } from '../controllers/login';

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

export const isLoggedIn = (req: Request, res: Response, next: NextFunction) => {
    if (!req.headers.authorization) {
        return res.status(401).json({error: 'unauthorized'})
    }

    const data = validateJWT(req.headers.authorization)

    if (!data) {
        return res.status(401).json({error: 'unauthorized'})
    }

    return next()
}

export const isUserType = (type: CompanyType): RequestHandler<any, any, any, {user: User}> => (req, res, next) => {
    if (!req.headers.authorization) {
        return res.status(401).json({error: "unauthorized"})
    }
    const isValidJWT = validateJWT(req.headers.authorization)

    if (!isValidJWT) {
        return res.status(403).json({error: "forbidden"})
    }

    return next()
    
}
