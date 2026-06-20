import tkinter as tk
from tkinter import messagebox
from PIL import Image, ImageDraw, ImageFont
from escpos.printer import Network
import os


class ReceiptPrinterApp:
    def __init__(self, root):
        self.root = root
        self.root.title("ESC/POS Printer Text to Image")
        self.root.geometry("450x450")  # UI 요소가 늘어남에 따라 창 크기 확장

        # --- UI 구성 ---
        setting_frame = tk.LabelFrame(
            root, text="프린터 및 폰트 설정", padx=10, pady=10
        )
        setting_frame.pack(fill="x", padx=10, pady=5)

        # 1. IP 주소
        tk.Label(setting_frame, text="IP 주소:").grid(
            row=0, column=0, sticky="w", pady=2
        )
        self.entry_ip = tk.Entry(setting_frame, width=15)
        self.entry_ip.insert(0, "192.168.219.191")
        self.entry_ip.grid(row=0, column=1, padx=5, pady=2)

        # 2. 포트
        tk.Label(setting_frame, text="Port:").grid(row=0, column=2, sticky="w", pady=2)
        self.entry_port = tk.Entry(setting_frame, width=6)
        self.entry_port.insert(0, "9100")
        self.entry_port.grid(row=0, column=3, padx=5, pady=2)

        # 3. 폰트 크기
        tk.Label(setting_frame, text="폰트 크기:").grid(
            row=1, column=0, sticky="w", pady=2
        )
        self.spin_fontsize = tk.Spinbox(setting_frame, from_=10, to=100, width=5)
        self.spin_fontsize.delete(0, "end")
        self.spin_fontsize.insert(0, "42")
        self.spin_fontsize.grid(row=1, column=1, sticky="w", padx=5, pady=2)
        tk.Label(setting_frame, text="px").grid(row=1, column=2, sticky="w")

        # 4. 용지 폭 (PAPER_WIDTH_PX) - 신규 추가
        tk.Label(setting_frame, text="용지 폭:").grid(
            row=2, column=0, sticky="w", pady=2
        )
        self.entry_width = tk.Entry(setting_frame, width=8)
        self.entry_width.insert(0, "576")  # 80mm 기본값
        self.entry_width.grid(row=2, column=1, sticky="w", padx=5, pady=2)
        tk.Label(setting_frame, text="px").grid(row=2, column=2, sticky="w")

        # 5. 폰트 파일명 - 신규 추가
        tk.Label(setting_frame, text="폰트 파일:").grid(
            row=3, column=0, sticky="w", pady=2
        )
        self.entry_fontname = tk.Entry(setting_frame, width=25)
        self.entry_fontname.insert(0, "IM_Hyemin-Bold.ttf")  # 기본 폰트명
        self.entry_fontname.grid(
            row=3, column=1, columnspan=3, sticky="w", padx=5, pady=2
        )

        # --- 텍스트 입력 영역 ---
        text_frame = tk.LabelFrame(root, text="출력할 내용", padx=10, pady=10)
        text_frame.pack(fill="both", expand=True, padx=10, pady=5)

        self.text_input = tk.Text(text_frame, height=15)
        self.text_input.pack(fill="both", expand=True)

        # --- 버튼 영역 ---
        btn_frame = tk.Frame(root, pady=10)
        btn_frame.pack(fill="x")

        self.btn_print = tk.Button(
            btn_frame,
            text="이미지로 변환 및 출력",
            command=self.process_and_print,
            bg="lightblue",
            height=2,
        )
        self.btn_print.pack(fill="x", padx=10)

    def get_wrapped_lines(self, text, font, max_width):
        """픽셀 단위로 너비를 계산하여 줄바꿈을 수행하는 함수"""
        lines = []
        for paragraph in text.split("\n"):
            current_line = ""
            for char in paragraph:
                # 글자 너비 측정
                if hasattr(font, "getlength"):
                    w = font.getlength(current_line + char)
                else:
                    w = font.getsize(current_line + char)[0]

                if w <= max_width:
                    current_line += char
                else:
                    lines.append(current_line)
                    current_line = char

            lines.append(current_line)
        return lines

    def text_to_image(self, text, font_size, paper_width, font_filename):
        """
        설정값을 인자로 받아 이미지를 생성합니다.
        font_filename: UI에서 입력한 폰트 파일명
        paper_width: UI에서 입력한 용지 폭(px)
        """
        # 폰트 경로 계산
        current_dir = os.path.dirname(os.path.abspath(__file__))
        font_path = os.path.join(current_dir, font_filename)

        try:
            font = ImageFont.truetype(font_path, font_size)
        except IOError:
            messagebox.showerror(
                "폰트 오류",
                f"폰트 파일을 찾을 수 없습니다.\n\n파일명: {font_filename}\n경로: {current_dir}\n\n파일이 같은 폴더에 있는지, 이름이 정확한지 확인해주세요.",
            )
            return None

        # 사용할 수 있는 최대 너비 (좌우 여백 10px씩 제외)
        safe_width_px = paper_width - 20

        # 줄바꿈 로직 (픽셀 기반)
        lines = self.get_wrapped_lines(text, font, safe_width_px)

        # 이미지 높이 계산
        line_spacing = int(font_size * 0.2)
        line_height = font_size + line_spacing
        total_lines = len(lines) if len(lines) > 0 else 1
        img_height = (total_lines * line_height) + 20

        # 이미지 생성 (너비는 입력받은 paper_width 사용)
        image = Image.new("RGB", (paper_width, img_height), "white")
        draw = ImageDraw.Draw(image)

        y_text = 10
        for line in lines:
            draw.text((10, y_text), line, font=font, fill="black")
            y_text += line_height

        return image

    def process_and_print(self):
        """UI 설정값으로 이미지 생성 후 출력"""
        # UI에서 값 가져오기
        ip = self.entry_ip.get()
        port_str = self.entry_port.get()
        font_size_str = self.spin_fontsize.get()
        width_str = self.entry_width.get()
        font_name = self.entry_fontname.get().strip()
        content = self.text_input.get("1.0", tk.END).strip()

        if not content:
            messagebox.showwarning("경고", "출력할 내용을 입력해주세요.")
            return

        if not font_name:
            messagebox.showwarning("경고", "폰트 파일명을 입력해주세요.")
            return

        try:
            port = int(port_str)
            font_size = int(font_size_str)
            paper_width = int(width_str)

            if font_size < 5:
                raise ValueError("폰트 크기가 너무 작습니다.")
            if paper_width < 100:
                raise ValueError("용지 폭이 너무 작습니다.")

        except ValueError:
            messagebox.showerror(
                "입력 오류", "포트, 폰트 크기, 용지 폭은 숫자여야 합니다."
            )
            return

        # 1. 이미지 변환 (입력된 설정값 전달)
        img = self.text_to_image(content, font_size, paper_width, font_name)
        if img is None:
            return

        # 2. 프린터 전송
        p = None
        try:
            p = Network(ip, port)
            p.image(img, impl="bitImageRaster")
            p.cut()
            messagebox.showinfo("성공", "출력이 완료되었습니다.")
        except Exception as e:
            messagebox.showerror(
                "출력 실패", f"프린터 통신 중 오류가 발생했습니다.\n\n{str(e)}"
            )
        finally:
            if p is not None:
                p.close()


if __name__ == "__main__":
    root = tk.Tk()
    app = ReceiptPrinterApp(root)
    root.mainloop()
