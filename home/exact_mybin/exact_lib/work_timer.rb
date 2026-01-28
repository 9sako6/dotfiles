module WorkTimer
  class Work
    def initialize(fileName)
      @fileName = fileName
    end

    def start_work(startTime=nil)
      @startTime ||= Time.now
    end

    def end_work(endTime=nil)
      @endTime ||= Time.now
      stop_view
    end

    # 戻り値はString
    def total_time
      totalTime = 0
      begin
        f = File.open(@fileName, "r")
      rescue
        return sec2seq(totalTime)
      end
      f.each_line do |line|
        next if line =~ /^\d{4}-\d{2}-\d{2}/
        # **:**:**表示を秒に変換
        line.chomp.split(":").each_with_index do |time, i|
          totalTime += time.to_i*60**(2-i)
        end
      end
      f.close
      sec2seq(totalTime)
    end

    def record_time
      File.open(@fileName, "a+") do |f|
        workTime = (@endTime.gmtime-@startTime.gmtime.to_i).strftime("%T")
        f.puts @startTime.localtime, @endTime.localtime, workTime
      end
    end

    def view
      @view_thread = Thread.new do
        loop do
          workingTime = Time.now - @startTime.to_i
          print "\r\033[32m#{(workingTime+60*60*15).strftime("%T")}\033[0m"
          sleep 1
        end
      end
      true
    end

    private

    def sec2seq(sec)
      hours = (sec/3600.0).to_i.to_s
      minutes = ((sec-(sec/3600.0).to_i*3600.0)/60).to_i.to_s
      seconds = (sec%60).to_i.to_s
      "#{two_digits(hours)}:#{two_digits(minutes)}:#{two_digits(seconds)}"
    end

    # 1桁の数字（実際はStringクラス）を2桁にする (e.g.: 0:0:47 -> 00:00:47)
    def two_digits(time)
      time.size==1 ? "0"+time : time
    end

    def stop_view
      @view_thread.kill if @view_thread && @view_thread.alive?
    end
  end
end
