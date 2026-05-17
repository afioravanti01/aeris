import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { api } from '../api/client';

const PRIORITIES = ['low', 'medium', 'high', 'critical'];

export default function TicketForm() {
  const navigate = useNavigate();
  const [users,   setUsers]   = useState([]);
  const [form,    setForm]    = useState({ title: '', description: '', priority: 'medium', assignee_id: '' });
  const [saving,  setSaving]  = useState(false);
  const [error,   setError]   = useState(null);

  useEffect(() => {
    api.users.list().then(r => setUsers(r.data)).catch(() => {});
  }, []);

  const set = (field) => (e) => setForm(f => ({ ...f, [field]: e.target.value }));

  const submit = async (e) => {
    e.preventDefault();
    if (!form.title.trim()) return setError('Title is required.');
    setSaving(true);
    try {
      const { data } = await api.tickets.create(form);
      navigate(`/tickets/${data.id}`);
    } catch (err) {
      setError(err.response?.data?.detail ?? err.message);
      setSaving(false);
    }
  };

  return (
    <form onSubmit={submit} style={{ maxWidth: 560 }}>
      <h2>New Ticket</h2>
      {error && <p style={{ color: 'red' }}>{error}</p>}

      <label>Title *<br />
        <input value={form.title} onChange={set('title')} style={{ width: '100%' }} />
      </label>

      <label style={{ display: 'block', marginTop: 12 }}>Description<br />
        <textarea value={form.description} onChange={set('description')} rows={5} style={{ width: '100%' }} />
      </label>

      <label style={{ display: 'block', marginTop: 12 }}>Priority<br />
        <select value={form.priority} onChange={set('priority')}>
          {PRIORITIES.map(p => <option key={p} value={p}>{p}</option>)}
        </select>
      </label>

      <label style={{ display: 'block', marginTop: 12 }}>Assign to<br />
        <select value={form.assignee_id} onChange={set('assignee_id')}>
          <option value="">— none —</option>
          {users.map(u => <option key={u.id} value={u.id}>{u.full_name}</option>)}
        </select>
      </label>

      <div style={{ marginTop: 20 }}>
        <button type="submit" disabled={saving}>{saving ? 'Saving...' : 'Create Ticket'}</button>
        <button type="button" onClick={() => navigate(-1)} style={{ marginLeft: 8 }}>Cancel</button>
      </div>
    </form>
  );
}
