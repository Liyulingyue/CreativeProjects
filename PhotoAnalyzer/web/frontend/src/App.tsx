import { Routes, Route } from "react-router-dom";
import { Layout } from "./components/Layout";
import { Dashboard } from "./pages/Dashboard";
import { Explorer } from "./pages/Explorer";
import { Analysis } from "./pages/Analysis";
import { Dedup } from "./pages/Dedup";
import { Settings } from "./pages/Settings";
import { Cache } from "./pages/Cache";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Dashboard />} />
        <Route path="/explorer" element={<Explorer />} />
        <Route path="/analysis" element={<Analysis />} />
        <Route path="/dedup" element={<Dedup />} />
        <Route path="/cache" element={<Cache />} />
        <Route path="/settings" element={<Settings />} />
      </Route>
    </Routes>
  );
}
