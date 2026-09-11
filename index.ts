import db from "./db"
import express from "express"
import app from "./api"

const PORT = process.env.PORT ?? 8000

app.listen(
    PORT,
    () => console.log(`Music Licensing Service listening on :${PORT}`)
)
