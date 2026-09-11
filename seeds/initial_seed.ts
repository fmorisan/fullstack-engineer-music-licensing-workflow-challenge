import type { Knex } from "knex";

export async function seed(knex: Knex): Promise<void> {
    await knex('users').del()
    await knex('companies').del()

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
            password_hash: '34adfa7497e65a614fdbb3426c1e7bf9:f561973413dd6642be8650c3d50b979f96d2c9493ef8b95f77147217cd9578dd82e273970be487bd532b9774617731775c4e7e470133abf3548bd9143ca9a5b9',
            employer: '849a4821-fb9a-4c6a-8fff-662cc37f6802'

        },
        {
            id: '1f616ea9-176d-461c-b14c-65241f81e3f2',
            username: 'mark.evil',
            email: 'mark@evilrecords.com',
            password_hash: 'f7c5590a19800d715da1d459b690926b:c63cec8cb78f5a09361e1e016ed69b5f5902d064c1dd524b880ec0de6824000665294faf42ab5aa2ad64a8f984f997e082114e4c8ed25fa8923dbad46b63dafe',
            employer: 'b03a0ef7-1ff8-43aa-9085-5c3f075e9666',
        }
    ])
};
