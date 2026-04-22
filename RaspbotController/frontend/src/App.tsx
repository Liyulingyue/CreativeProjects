import { useState } from 'react';
import { Layout } from './components/Layout';
import { RemotePage } from './pages/RemotePage';
import { DashboardPage } from './pages/DashboardPage';
import { SettingsPage } from './pages/SettingsPage';
import './App.css';

function App() {
  const [activePage, setActivePage] = useState('remote');

  const renderPage = () => {
    switch (activePage) {
      case 'remote':
        return <RemotePage />;
      case 'dashboard':
        return <DashboardPage />;
      case 'settings':
        return <SettingsPage />;
      default:
        return <RemotePage />;
    }
  };

  return (
    <Layout activePage={activePage} onNavigate={setActivePage}>
      {renderPage()}
    </Layout>
  );
}

export default App;
