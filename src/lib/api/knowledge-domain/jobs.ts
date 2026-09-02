import { knowledgeApi } from "../knowledge";

export const knowledgeJobsApi = {
  list: knowledgeApi.listJobs,
  get: knowledgeApi.getJob,
  cancel: knowledgeApi.cancelJob,
  retry: knowledgeApi.retryJob,
};
