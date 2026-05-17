import axios from 'axios';

const BASE = '/api';

export const api = {
  tickets: {
    list:   (params = {}) => axios.get(`${BASE}/tickets`, { params }),
    get:    (id)          => axios.get(`${BASE}/tickets/${id}`),
    create: (data)        => axios.post(`${BASE}/tickets`, data),
    update: (id, data)    => axios.put(`${BASE}/tickets/${id}`, data),
    close:  (id)          => axios.patch(`${BASE}/tickets/${id}/close`),
    comment:(id, text)    => axios.post(`${BASE}/tickets/${id}/comments`, { text }),
  },
  users: {
    list: ()   => axios.get(`${BASE}/users`),
    me:   ()   => axios.get(`${BASE}/users/me`),
  },
};
