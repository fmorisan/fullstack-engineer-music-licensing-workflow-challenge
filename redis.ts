import { createClient, type RedisClientType } from 'redis'

const REDIS_URL = process.env.REDIS_URL

let publisher: RedisClientType | null = null
let subscriber: RedisClientType | null = null

const connect = async (url: string) => {
    const client = createClient({ url })
    client.on('error', (err) => console.error('redis error:', err.message))
    await client.connect()
    return client
}

export const getPublisher = async () => {
    if (!REDIS_URL) return null
    if (!publisher) publisher = await connect(REDIS_URL)
    return publisher
}

export const getSubscriber = async () => {
    if (!REDIS_URL) return null
    if (!subscriber) subscriber = await connect(REDIS_URL)
    return subscriber
}

export const probeRedis = async (timeoutMs = 1000): Promise<boolean> => {
    if (!REDIS_URL) return false
    try {
        const client = await Promise.race([
            getPublisher(),
            new Promise<null>(resolve => setTimeout(() => resolve(null), timeoutMs))
        ])
        if (!client) return false
        await Promise.race([
            client.ping(),
            new Promise<never>((_, reject) => setTimeout(() => reject(new Error('redis probe timeout')), timeoutMs))
        ])
        return true
    } catch {
        return false
    }
}
