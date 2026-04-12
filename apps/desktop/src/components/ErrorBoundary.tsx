import { Component, type ReactNode } from "react";
import { Button } from "./ui/Button";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  handleRetry = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div className="error-boundary" role="alert">
          <div className="error-boundary-content">
            <h2 className="error-boundary-title">出了点问题</h2>
            <p className="error-boundary-message">
              {this.state.error?.message ?? "未知错误"}
            </p>
            <Button variant="primary" onClick={this.handleRetry} aria-label="重试">
              重试
            </Button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
