import unittest
from retrieval_benchmark import score
class EvidenceScoringTests(unittest.TestCase):
    def run_case(self, hit, ordered=False):
        q={"id":"Q1","query":"VN-5103","category":"video","expected_files":["a.mp4"],"exact_match":not ordered,"ordered":ordered,"time_ranges":[{"file":"a.mp4","start_seconds":8,"end_seconds":16,"modality":"visual"}]}
        if ordered: q["time_ranges"].append({"file":"a.mp4","start_seconds":17,"end_seconds":25,"modality":"visual"})
        return score({"files":[],"queries":[q]},[{"query":{"id":"Q1"},"response":{"results":[hit]}}])["overall"]
    def test_correct_file_wrong_audio_is_not_evidence_success(self):
        r=self.run_case({"path":"a.mp4","kind":"video_audio","seconds":0})
        self.assertEqual(r["top1"],1);self.assertEqual(r["evidence_top1"],0)
    def test_visual_identifier_requires_actual_text_at_the_correct_time(self):
        for text, expected in [("VN-5103",1),("unrelated",0)]:
            r=self.run_case({"path":"a.mp4","kind":"video","text":text,"seconds":10})
            self.assertEqual(r["evidence_top1"],expected)
    def test_sequence_requires_both_moments(self):
        h={"path":"a.mp4","evidence":[{"source":"video_frame","seconds":10},{"source":"video_frame","seconds":20}]}
        self.assertEqual(self.run_case(h,True)["evidence_top1"],1)
        h["evidence"].pop()
        self.assertEqual(self.run_case(h,True)["evidence_top1"],0)
    def test_universal_scoring_does_not_remove_wrong_category_distractors(self):
        q={"id":"Q1","category":"video","expected_files":["a.mp4"]}
        truth={"files":[{"file":"a.mp4","category":"video"},{"file":"b.txt","category":"text"}],"queries":[q]}
        raw=[{"query":{"id":"Q1"},"response":{"results":[{"path":"b.txt"},{"path":"a.mp4"}]}}]
        self.assertEqual(score(truth,raw)["overall"]["top1"],0)
        self.assertEqual(score(truth,raw,True)["overall"]["top1"],1)
    def test_page_and_exact_text_are_scored_separately(self):
        q={"id":"Q1","query":"INV-123","expected_files":["a.pdf"],"exact_match":True,"page_ranges":[{"file":"a.pdf","start_page":3,"end_page":3}]}
        truth={"files":[],"queries":[q]}
        raw=[{"query":{"id":"Q1"},"response":{"results":[{"path":"a.pdf","page":1,"text":"INV-123"}]}}]
        r=score(truth,raw)["overall"]
        self.assertEqual(r["components"]["exact_text"]["top1"],1)
        self.assertEqual(r["components"]["page"]["top1"],0)
        self.assertEqual(r["evidence_top1"],0)
    def test_missing_or_duplicate_queries_are_errors(self):
        with self.assertRaises(ValueError):score({"files":[],"queries":[{"id":"Q1"}]},[])
if __name__ == "__main__":unittest.main()
