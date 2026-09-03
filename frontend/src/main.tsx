import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createBrowserRouter, Navigate, RouterProvider } from "react-router-dom";
import { AuthProvider, useAuth } from "./auth/AuthContext";
import { Layout } from "./components/Layout";
import { LoginPage } from "./pages/LoginPage";
import { MoviesPage } from "./pages/MoviesPage";
import { MovieDetailPage } from "./pages/MovieDetailPage";
import { SearchPage } from "./pages/SearchPage";
import "./index.css";

const Home = () => {
  const { user } = useAuth();
  return (
    <div className="page-placeholder">
      <h2>Welcome, {user?.display_name ?? "stranger"}.</h2>
      <p>Views land with the next commits.</p>
    </div>
  );
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 5_000, refetchOnWindowFocus: true },
  },
});

const Protected = ({ children }: { children: React.ReactNode }) => {
  const { user, ready } = useAuth();
  if (!ready) return <div className="loading">Loading…</div>;
  if (!user) return <Navigate to="/login" replace />;
  return <>{children}</>;
};

const router = createBrowserRouter([
  {
    path: "/login",
    element: <LoginPage />,
  },
  {
    path: "/",
    element: (
      <Protected>
        <Layout />
      </Protected>
    ),
    children: [
      { index: true, element: <Home /> },
      { path: "movies", element: <MoviesPage /> },
      { path: "movies/:movieId", element: <MovieDetailPage /> },
      { path: "search", element: <SearchPage /> },
    ],
  },
]);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <AuthProvider>
        <RouterProvider router={router} />
      </AuthProvider>
    </QueryClientProvider>
  </StrictMode>,
);
