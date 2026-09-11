import db from "./db"
import express from "express"
import app from "./api"

app.listen(process.env.PORT ?? 8000, () => console.log('listening on 8000'))
