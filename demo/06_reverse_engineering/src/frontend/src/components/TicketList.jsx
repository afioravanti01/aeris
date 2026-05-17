import React, { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { api } from '../api/client';

const PRIORITY_COLOR = { low: '#6b7280', medium: '#d97706', high: '#dc2626', critical: '#7c3aed' };
const STATUS_LABEL   = { open: 'Open', in_progress: 'In progress', resolved: 'Resolved', closed: 'Closed' };

export default function TicketList() {
  const [tickets, setTickets]   = useState([]);
  const [filter,  setFilter]    = useState({ status: '', priority: '' });
  const [loading, setLoading]   = useState(true);
  const [error,   setError]     = useState(null);

  useEffect(() => {
    setLoading(true);
    api.tickets.list(filter)
      .then(r => setTickets(r.data))
      .catch(e => setError(e.message))
      .finally(() => setLoading(false));
  }, [filter]);

  if (loading) return <p>Loading...</p>;
  if (error)   return <p style={{ color: 'red' }}>Error: {error}</p>;

  return (
    <div>
      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        <select value={filter.status} onChange={e => setFilter(f => ({ ...f, status: e.target.value }))}>
          <option value="">All statuses</option>
          {Object.entries(STATUS_LABEL).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
        </select>
        <select value={filter.priority} onChange={e => setFilter(f => ({ ...f, priority: e.target.value }))}>
          <option value="">All priorities</option>
          {Object.keys(PRIORITY_COLOR).map(p => <option key={p} value={p}>{p}</option>)}
        </select>
        <Link to="/new"><button>+ New ticket</button></Link>
      </div>

      {tickets.length === 0 && <p>No tickets found.</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ borderBottom: '2px solid #e5e7eb' }}>
            <th>#</th><th>Title</th><th>Priority</th><th>Status</th><th>Assignee</th><th>Created</th>
          </tr>
        </thead>
        <tbody>
          {tickets.map(t => (
            <tr key={t.id} style={{ borderBottom: '1px solid #f3f4f6' }}>
              <td><Link to={`/tickets/${t.id}`}>{t.id}</Link></td>
              <td>{t.title}</td>
              <td><span style={{ color: PRIORITY_COLOR[t.priority] }}>{t.priority}</span></td>
              <td>{STATUS_LABEL[t.status] ?? t.status}</td>
              <td>{t.assignee ?? '—'}</td>
              <td>{new Date(t.created_at).toLocaleDateString('en-GB')}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
