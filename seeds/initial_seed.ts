import type { Knex } from "knex";

export async function seed(knex: Knex): Promise<void> {

    await knex('companies').insert([
        {
            id: '849a4821-fb9a-4c6a-8fff-662cc37f6802',
            name: 'Gotham Studios',
            kind: 'movie_studio'
        },
        {
            id: 'b03a0ef7-1ff8-43aa-9085-5c3f075e9666',
            name: 'Evil Records',
            kind: 'record_label'
        }
    ])

    await knex('users').insert([
        {
            id: '9795f84a-6836-4e13-84e1-7a1db8bd24d0',
            username: 'gracegs',
            email: 'grace@gotham.studios',
            password_hash: '',
            employer: '849a4821-fb9a-4c6a-8fff-662cc37f6802'

        },
        {
            id: '1f616ea9-176d-461c-b14c-65241f81e3f2',
            username: 'mark.evil',
            email: 'mark@evilrecords.com',
            password_hash: '',
            employer: 'b03a0ef7-1ff8-43aa-9085-5c3f075e9666',
        }
    ])
};
