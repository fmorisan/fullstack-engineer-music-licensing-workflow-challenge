import type { Knex } from "knex";


export async function up(knex: Knex): Promise<void> {
    await knex.schema.createTable('refresh_tokens', t => {
        t.uuid('id').primary().notNullable()

        t.uuid('user_id').notNullable().index()
        t.foreign('user_id').references('users.id').onDelete('CASCADE')

        t.string('token', 64).notNullable().unique()

        t.timestamp('expires_at').notNullable()
        t.timestamp('created_at').notNullable().defaultTo(knex.fn.now())
    })
}


export async function down(knex: Knex): Promise<void> {
    await knex.schema.dropTableIfExists('refresh_tokens')
}

