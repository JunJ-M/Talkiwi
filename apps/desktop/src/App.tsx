import "./styles/global.css";
import "./components/Shell/shell.css";
import { Shell } from "./components/Shell/Shell";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ToastContainer } from "./components/ui/Toast";

function App() {
  return (
    <ErrorBoundary>
      <Shell />
      <ToastContainer />
    </ErrorBoundary>
  );
}

export default App;
