import type { Knex } from "knex";


export async function up(knex: Knex): Promise<void> {
    await knex.schema.createTable('companies', t => {
        t.uuid('id').primary().notNullable()
        t.string('name').notNullable()
        t.enu('kind', ['movie_studio', 'record_label'])
    })

    await knex.schema.createTable('users', t => {
        t.uuid('id').primary().notNullable()
        t.string('username').unique().notNullable()
        t.string('email').unique().notNullable()
        t.string('password_hash').notNullable()
        t.uuid('employer').notNullable()
        t.foreign('employer').references('companies.id').onDelete('CASCADE')
    })

    await knex.schema.createTable('movies', t => {
        t.uuid('id').primary().notNullable()
        t.string('name').notNullable()
        t.string('description', 300).notNullable()
        t.string('poster', 100)
        t.uuid('studio_id').notNullable()
        t.foreign('studio_id').references('companies.id').onDelete('CASCADE')
    })

    await knex.schema.createTable('songs', t => {
        t.uuid('id').primary().notNullable()
        t.string('name').notNullable()
        t.string('author').notNullable()
        t.string('boxart', 100)
        t.uuid('label_id').notNullable().index()
        t.foreign('label_id').references('companies.id').onDelete('CASCADE')
    })

    await knex.schema.createTable('movie_scenes', t => {
        t.uuid('id').primary().notNullable()

        t.uuid('movie_id').notNullable().index()
        t.foreign('movie_id').references('movies.id').onDelete('CASCADE')

        t.string('name').notNullable()
        t.integer('scene_number').notNullable()
        t.integer('start_offset').notNullable()
        t.integer('length').notNullable()

        t.unique(['movie_id', 'scene_number'])
    })

    await knex.schema.createTable('licenses', t => {
        t.uuid('id').primary().notNullable()
        t.uuid('scene_id').notNullable().index()
        t.uuid('song_id').notNullable().index()

        t.integer('start_offset').notNullable()
        t.integer('length').notNullable()

        t.integer('license_fee').notNullable()
        t.enu('state', ['OFFER', 'COUNTER', 'REJECTED', 'ACCEPTED']).defaultTo('OFFER').notNullable()
    })
}


export async function down(knex: Knex): Promise<void> {
    await knex.schema.dropTableIfExists('users')
    await knex.schema.dropTableIfExists('companies')
    await knex.schema.dropTableIfExists('movies')
    await knex.schema.dropTableIfExists('songs')
    await knex.schema.dropTableIfExists('movie_scenes')
    await knex.schema.dropTableIfExists('licenses')
}

