import { BrowserRouter, Routes, Route } from "react-router-dom";
import { MainLayout } from "@/components/layout/MainLayout";
import { Dashboard } from "@/pages/Dashboard";
import { NewPublish } from "@/pages/NewPublish";
import { Accounts } from "@/pages/Accounts";
import { Templates } from "@/pages/Templates";
import { History } from "@/pages/History";
import { SettingsPage } from "@/pages/SettingsPage";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<MainLayout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/publish" element={<NewPublish />} />
          <Route path="/accounts" element={<Accounts />} />
          <Route path="/templates" element={<Templates />} />
          <Route path="/history" element={<History />} />
          <Route path="/settings" element={<SettingsPage />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}

export default App;
