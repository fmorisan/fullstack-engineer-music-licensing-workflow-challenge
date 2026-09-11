import type { Knex } from "knex";


export async function up(knex: Knex): Promise<void> {
    await knex.schema.alterTable('songs', t => {
        t.integer('length_seconds').notNullable().defaultTo(0)
    })
}


export async function down(knex: Knex): Promise<void> {
    await knex.schema.alterTable('songs', t => {
        t.dropColumn('length_seconds')
    })
}

