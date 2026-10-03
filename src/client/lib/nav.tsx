import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";

const NavigateContext = createContext<(to: string) => void>(() => {
  window.location.assign("/");
});

export function PathProvider({ children }: { children: ReactNode }) {
  const [path, setPath] = useState(() => window.location.pathname);

  useEffect(() => {
    const onPop = () => setPath(window.location.pathname);
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  const go = useCallback((to: string) => {
    window.history.pushState({}, "", to);
    setPath(to);
  }, []);

  return (
    <NavigateContext.Provider value={go}>
      <PathContext.Provider value={path}>{children}</PathContext.Provider>
    </NavigateContext.Provider>
  );
}

const PathContext = createContext("/");

export function usePath(): string {
  return useContext(PathContext);
}

export function useNavigate(): (to: string) => void {
  return useContext(NavigateContext);
}
