import { createContext } from "react";
import { api, type AppApi } from "@/lib/api";

export const IconBackendContext = createContext<AppApi>(api);
