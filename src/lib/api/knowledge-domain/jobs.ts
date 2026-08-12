import { knowledgeApi } from "../knowledge";

export const knowledgeJobsApi = {
  list: knowledgeApi.listJobs,
  cancel: knowledgeApi.cancelJob,
  retry: knowledgeApi.retryJob,
};
